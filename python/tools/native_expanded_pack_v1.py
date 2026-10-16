"""Side-by-side native-expanded training runs inside one held host reservation.

qualify PLAN measures a pack: a family's one-update qualification payload, with
one exact configuration and seeds, alone on one slot of each kind (one after
another), then on every slot at once. The packed trials wait at a start barrier
after their checks and start together. It writes the pack index
(native_expanded_dispatch_v1.PACK) that a choice's `packs` cites.

run PLAN keeps one production dispatch on every slot of the members' selected
pack until the members are done, starting each member on the first free slot,
and runs an optional post command per member (a trace) on the plan's post CPUs,
declared beside the slots, so it overlaps the next member.

Every trial and member goes through native_expanded_dispatch_v1 (nested in the
held reservation), which admits each one as it does any other dispatch; this
tool only places, starts and waits. docs/native_expanded_dispatch_v1.md, "Packs".
"""
from __future__ import annotations

import argparse
import copy
import ctypes
import json
import os
import queue
import re
import signal
import subprocess
import sys
import threading
import time
from pathlib import Path

import host_reservation_v1 as reservations
import host_slots_v1 as slots
import native_expanded_dispatch_v1 as dispatch
from public_training_dispatch_v2 import checked, pin, read, write

QUALIFY = "native-expanded-pack-qualify/v1"
RUN = "native-expanded-pack-run/v1"
DISPATCHER = str(Path(dispatch.__file__).resolve())
POLL_SECONDS = 5.0
POST_POLL_SECONDS = 0.2
POST_TIMEOUT_CODE = 124
CREATE_SUSPENDED = 0x4
# A busy refusal spawns nothing: other work on the host ends, and a sibling of
# this reservation caught before it was recorded is exempt on the next look
# (as nine_deck_eval_driver_v1 waits it out).
BUSY_RETRIES, BUSY_RETRY_SECONDS = 60, 60
SLOT_KEYS = ("cpu_affinity", "workers", "preparation_workers", "memory_bytes")
NAME = re.compile(r"[A-Za-z0-9._-]+")
require = dispatch.require


def absolute(path) -> Path:
    path = Path(path)
    require(path.is_absolute(), "absolute paths required")
    return path


def plan_slots(plan, host):
    """The plan's slots with their host: disjoint CPU sets, workers within each
    slot's CPUs."""
    found = plan["slots"]
    require(found and all(sorted(slot) == sorted(SLOT_KEYS) for slot in found), "invalid slots")
    out = []
    for slot in found:
        cpus = slot["cpu_affinity"]
        require(cpus and all(type(cpu) is int and cpu >= 0 for cpu in cpus) and len(set(cpus)) == len(cpus),
                "invalid slot CPU set")
        for key in ("workers", "preparation_workers"):
            require(type(slot[key]) is int and 1 <= slot[key] <= len(cpus), "slot workers exceed its CPUs")
        require(type(slot["memory_bytes"]) is int and slot["memory_bytes"] > 0, "invalid slot memory limit")
        out.append({"host": host, **{key: slot[key] for key in SLOT_KEYS}})
    every = [cpu for slot in out for cpu in slot["cpu_affinity"]]
    require(len(every) == len(set(every)), "slots overlap")
    require(len({shape(slot) for slot in out}) == 1, "a pack's slots must share one worker count")
    return out


def declared_cores():
    """Cores this process may place work on: the held reservation's timed declaration."""
    token = os.environ.get(slots.TOKEN_ENV)
    require(token and reservations.status(token).get("token_fate") == "holds", "held host reservation required")
    timed = slots.timed_state()
    require(timed.get("token_sha256") == slots.token_hash(token) and timed.get("cores") is not None,
            "the reservation must declare its timed cores (host_slots_v1.py timed)")
    return set(timed["cores"])


def host_load():
    """The reservation's timed declaration and every live host_slots claim, which
    the pack records around its trials (dispatch.pack_load reads them)."""
    timed = slots.timed_state()
    with slots.Mutex():
        claims = slots.live_claims()
    return {"at_unix": time.time(), "token_sha256": timed.get("token_sha256"), "cores": timed.get("cores"),
            "claims": [{"id": claim.get("id"), "cores": sorted(claim["cores"]), "state": claim.get("state"),
                        "lane": claim.get("lane")} for claim in claims]}


def available_memory() -> int:
    if os.name == "nt":
        class Status(ctypes.Structure):
            _fields_ = [("length", ctypes.c_ulong), ("load", ctypes.c_ulong)] + [
                (name, ctypes.c_ulonglong) for name in ("total", "available", "total_page", "available_page",
                                                         "total_virtual", "available_virtual", "extended")]
        status = Status()
        status.length = ctypes.sizeof(status)
        require(ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)), "cannot read free memory")
        return status.available
    for line in Path("/proc/meminfo").read_text().splitlines():
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) * 1024
    raise ValueError("cannot read free memory")


def physical_cores(topology):
    """Each logical CPU's physical core: the set of its SMT siblings."""
    owner = {}
    for row in topology:
        cpus = frozenset(slots.parse_cores(row["cpus"]))
        for cpu in cpus:
            owner[cpu] = cpus
    return owner


def admit(slots_, post_cpus=(), cores=None, memory=None, topology=None):
    """Slots and post CPUs inside the declared cores; every SMT sibling of a slot
    CPU declared and none of them a post CPU, so no work outside the slots shares
    a slot's physical cores; the slots' memory limits inside free memory."""
    cores = declared_cores() if cores is None else cores
    used, post = {cpu for slot in slots_ for cpu in slot["cpu_affinity"]}, set(post_cpus)
    require(used <= cores and post <= cores, "slot or post CPU outside the declared cores")
    owner = physical_cores(dispatch.host_topology() if topology is None else topology)
    require(used | post <= set(owner), "CPU outside this host's topology")
    siblings = set().union(*(owner[cpu] for cpu in used))
    require(siblings <= cores, "a slot's SMT sibling is not declared")
    require(not siblings & post, "a post CPU shares a physical core with a slot")
    memory = available_memory() if memory is None else memory
    require(sum(slot["memory_bytes"] for slot in slots_) <= memory, "slot memory limits exceed free memory")


def on_slot(request, slot):
    """`request` with its placement replaced by `slot`; nothing else changes."""
    value = copy.deepcopy(request)
    value["placement"] = dict(slot)
    return value


def trial(request, slot, name, tag):
    """A trial of the base qualification request on `slot`, with its own hot and
    recovery roots beside the base request's, named by the plan (`tag`) and trial."""
    value = on_slot(request, slot)
    for key in ("root", "cold_root"):
        path = Path(request[key])
        value[key] = str(path.with_name(f"{path.name}-{tag}-{name}")).replace("\\", "/")
    return value


def start(action, request_path, compute_host_name=None, retries=BUSY_RETRIES, pause=BUSY_RETRY_SECONDS):
    """One nested dispatch through the guarded launcher. Returns its supervisor's
    pid once the supervisor is created and handed off. Busy refusals are retried."""
    # WMI-created supervisors inherit no environment: the host name travels on the command line.
    extra = ["--compute-host-name", compute_host_name] if compute_host_name else []
    for attempt in range(retries):
        done = subprocess.run([sys.executable, "-B", DISPATCHER, action, str(request_path), *extra],
                              capture_output=True, text=True, check=False)
        if done.returncode == 0 or "busy refusal" not in done.stderr or attempt == retries - 1:
            break
        time.sleep(pause)
    require(done.returncode == 0, f"{action} {request_path}: {done.stderr[-2000:]}")
    lines = done.stdout.strip().splitlines()
    result = json.loads(lines[-1]) if lines else {}
    require(result.get("nested") is True and result.get("state") == "dispatched",
            f"{action} {request_path}: {result}")
    return result["pid"]


class Supervisor:
    """A nested dispatch's supervisor, named by pid and creation time from its handoff."""

    def __init__(self, request, pid):
        self.token = os.environ.get(reservations.TOKEN_ENV)
        self.root = Path(request["root"])
        events, _ = reservations.read_events(self.token)
        handoff = next((event for event in reversed(events) if event.get("pid") == pid
                        and event.get("kind") in ("handoff", "handoff-unverified")), None)
        require(handoff is not None and handoff["kind"] == "handoff",
                f"{self.root}: supervisor {pid} ended before its handoff")
        self.identity = (pid, handoff["creation_time"])

    def done(self):
        """The receipt's path once the supervisor has recorded success; None while it
        runs. It records that it is done after its whole job (so its slot) has
        exited. A recorded failure, success without a receipt, and a supervisor
        that has exited (or whose pid names another process) without the record fail."""
        # Liveness first: a supervisor gone before this read that left no record never will.
        gone = reservations.process_state(*self.identity) == "absent"
        events, _ = reservations.read_events(self.token)
        done = next((event for event in events if event.get("kind") == "nested-done"
                     and (event.get("by_pid"), event.get("by_process_creation_time")) == self.identity), None)
        if done is not None:
            report = self.root / "report.json"
            if done.get("outcome") == "success" and report.exists():
                return report
            execution = self.root / "execution.json"
            error = read(execution).get("error") if execution.exists() else None
            raise RuntimeError(f"{self.root}: {done.get('outcome')} ({done.get('detail')}); {error}")
        require(not gone, f"{self.root}: supervisor {self.identity[0]} ended without recording that it was done")
        return None


def wait(request, supervisor, timeout, poll=POLL_SECONDS):
    """The receipt of `request` once its nested supervisor has recorded that it is done."""
    watched = Supervisor(request, supervisor)
    deadline = time.monotonic() + timeout
    while (report := watched.done()) is None:
        if time.monotonic() > deadline:
            raise TimeoutError(f"{watched.root}: not done within {timeout} s")
        time.sleep(poll)
    return report


def await_ready(trials, timeout, poll=POLL_SECONDS):
    """Waits until every packed trial (request, supervisor pid) waits at its start
    barrier. A trial that ends first, waiting or not, fails the wait."""
    watched = [(request, Supervisor(request, pid)) for request, pid in trials]
    deadline = time.monotonic() + timeout
    while True:
        for _, supervisor in watched:
            require(supervisor.done() is None, f"{supervisor.root}: ended before its start barrier")
        waiting = [request for request, _ in watched if not Path(request["start_barrier"]["ready"]).exists()]
        if not waiting:
            return
        if time.monotonic() > deadline:
            raise TimeoutError(f"{len(waiting)} packed trials not at their start barrier within {timeout} s")
        time.sleep(poll)


def release(path, go):
    """Starts (go) or fails (not go) every trial at the start barrier: a complete
    file renamed into place, so a reader never sees part of it."""
    path = Path(path)
    partial = path.with_name(path.name + ".partial")
    partial.write_text(json.dumps({"go": go}), encoding="utf-8")
    os.replace(partial, path)


def trial_record(slot, request, report_path):
    report = read(report_path)
    started = Path(request["root"]) / "started.json"
    require(read(started)["request"] == report["request"], "start record belongs to another request")
    return {"slot": slot, "report": pin(report_path), "started": pin(started)}


def host_arguments(plan):
    name = plan.get("compute_host_name")
    require(name is None or type(name) is str and name, "invalid compute host name")
    return {"compute_host_name": name} if name else {}


def summed_storage(storages):
    """One storage admission for work that writes at once: the first block's roots,
    reserve and allowance, with every block's projections added."""
    total = copy.deepcopy(storages[0])
    for other in storages[1:]:
        total["projected_additional_bytes"] += other["projected_additional_bytes"]
        for volume, value in other["projected_volume_bytes"].items():
            total["projected_volume_bytes"][volume] = total["projected_volume_bytes"].get(volume, 0) + value
    return total


def qualify(plan_path, start_=start, wait_=wait, admit_=admit, topology_=None, ready_=await_ready,
            storage_=None, load_=host_load):
    plan = read(plan_path)
    require(plan["schema"] == QUALIFY, "unsupported pack qualification plan")
    base = read(checked(plan["base_request"]))
    require(base["schema"] == dispatch.SCHEMA and base["kind"] == "training" and "choice" not in base
            and "start_barrier" not in base, "a pack qualifies from a training qualification request")
    host = host_arguments(plan)
    pack_slots = plan_slots(plan, base["placement"]["host"])
    admit_(pack_slots)
    topology = dispatch.host_topology() if topology_ is None else topology_
    kinds = dispatch.slot_kinds(pack_slots, topology)
    root = absolute(plan["root"])
    root.mkdir(parents=True)
    timeout = dispatch.positive(plan["timeout_seconds"], "trial timeout")
    # A packed trial waits at its barrier while the later ones start, each of which
    # may retry busy refusals.
    barrier_timeout = timeout + len(pack_slots) * BUSY_RETRIES * BUSY_RETRY_SECONDS
    tag = pin(plan_path)["sha256"][:12]
    solo = [kinds.index(kind) for kind in dict.fromkeys(kinds)]
    requests = {}
    for name, slot in [(f"solo-{i}", i) for i in solo] + [(f"packed-{i}", i) for i in range(len(pack_slots))]:
        requests[name] = trial(base, pack_slots[slot], name, tag)
        if name.startswith("packed-"):  # packed trials start together, after all their checks
            requests[name]["start_barrier"] = {"ready": (root / f"{name}.ready.json").as_posix(),
                                               "release": (root / "release.json").as_posix(),
                                               "timeout_seconds": barrier_timeout}
        write(root / f"{name}.request.json", requests[name])
    family = dispatch.workload(read(checked(base["config"])), "training", family=base.get("schedule_family"))
    # The declaration and the host's claims before the solo trials, at the packed
    # trials' release and after them.
    index = {"schema": dispatch.PACK, "workload": family, "slots": pack_slots, "kinds": kinds,
             "topology": topology, "plan": pin(plan_path), "load": {"start": load_()}, "solo": [], "packed": []}
    for i in solo:  # one after another, each alone
        name = f"solo-{i}"
        supervisor = start_("qualify", root / f"{name}.request.json", **host)
        index["solo"].append(trial_record(i, requests[name], wait_(requests[name], supervisor, timeout)))
    names = [f"packed-{i}" for i in range(len(pack_slots))]
    # Each trial admits its own storage before any of them writes; together they must fit.
    (storage_ or dispatch.validate_storage)(summed_storage([requests[name]["storage"] for name in names]))
    supervisors = []
    try:
        for name in names:
            supervisors.append(start_("qualify", root / f"{name}.request.json", **host))
        ready_([(requests[name], supervisor) for name, supervisor in zip(names, supervisors)], timeout)
        index["load"]["packed"] = load_()
    except BaseException:
        release(root / "release.json", False)  # trials already waiting fail at once
        raise
    release(root / "release.json", True)
    reports = [wait_(requests[name], supervisor, timeout) for name, supervisor in zip(names, supervisors)]
    index["packed"] = [trial_record(i, requests[name], path) for i, (name, path) in enumerate(zip(names, reports))]
    index["load"]["end"] = load_()
    write(root / "pack.json", index)
    summary = summarize(index)
    print(json.dumps({"pack": pin(root / "pack.json"), **summary}))
    return summary


def summarize(index):
    """Per-trial seconds, whether every output matched, the packed trials' start
    spread and the mean packed/solo seconds per kind."""
    members = index["solo"] + index["packed"]
    reports = [read(checked(member["report"])) for member in members]
    seconds = [report["seconds"] for report in reports]
    solo = {index["kinds"][member["slot"]]: value for member, value in zip(index["solo"], seconds)}
    packed = seconds[len(index["solo"]):]
    kinds = index["kinds"]
    starts = [read(checked(member["started"]))["started_unix"] for member in index["packed"]]
    return {"trial_seconds": seconds,
            "outputs_identical": all(report["fingerprint"] == reports[0]["fingerprint"] for report in reports),
            "packed_start_spread_seconds": max(starts) - min(starts),
            "slowdown": {kind: sum(v for v, k in zip(packed, kinds) if k == kind) / kinds.count(kind) / solo[kind]
                         for kind in sorted(set(kinds))}}


def shape(placement):
    """The worker counts a member's configuration is bound to."""
    return placement["workers"], placement["preparation_workers"]


def run_post(cpu, command, timeout):
    """A post command and every process it starts on one post CPU at below-normal
    priority, inside the reservation's declared cores, all stopped after `timeout`
    seconds. Returns its exit code (POST_TIMEOUT_CODE when it was stopped)."""
    return subprocess.run([sys.executable, "-B", str(Path(__file__).resolve()), "_post", str(cpu), str(timeout),
                           "--", *command], check=False).returncode


class Runner:
    """One production dispatch per slot until the queue is empty, a member fails or STOP.

    A member is a complete production request whose placement is one of its
    pack's slots, which share one worker count; it runs on the first free slot
    (its placement rewritten to that slot) with its own roots. The launcher
    admits each member against its choice. Post commands queue for the post CPUs."""

    def __init__(self, plan, root, start_=start, wait_=wait, post_=run_post, storage_=None, poll=POLL_SECONDS):
        self.plan, self.root, self.poll = plan, root, poll
        self.start_, self.wait_, self.post_ = start_, wait_, post_
        self.storage_ = storage_ or dispatch.validate_storage
        self.host = host_arguments(plan)
        self.lock = threading.Lock()
        self.queue = list(plan["members"])
        self.active = {}
        self.failures = []
        self.posts = queue.Queue()
        self.results = {}

    @property
    def failed(self):
        return self.failures[0] if self.failures else None

    def fail(self, text):
        with self.lock:
            self.failures.append(text)

    def log(self, **event):
        event["at_unix"] = time.time()
        with self.lock, (self.root / "log.jsonl").open("a", encoding="utf-8") as stream:
            stream.write(json.dumps(event) + "\n")

    def stopped(self):
        return bool(self.failures) or any(Path(path).exists() for path in self.plan["stop_files"])

    def storage_room(self, request):
        """The launcher's storage admission for `request` with the projections of the
        members already running added to its own: each volume keeps its reserve
        and the allowance holds retained bytes plus every projection. False
        while running members make it fail; a member that fails alone fails."""
        running = [other["storage"] for other in self.active.values()]
        try:
            self.storage_(summed_storage([request["storage"], *running]))
        except ValueError:
            if not running:
                raise
            return False
        return True

    def next_member(self, slot_index):
        """The first queued member, moved onto this slot; "wait" while the storage has
        no room for it yet; None when this slot is done."""
        with self.lock:
            if self.failures or any(Path(path).exists() for path in self.plan["stop_files"]) or not self.queue:
                return None
            member = self.queue[0]
            try:
                request = on_slot(read(checked(member["request"])), self.slots[slot_index])
                if not self.storage_room(request):
                    return "wait"
            except BaseException as error:
                self.failures.append(f"{member['name']}: {error}")
                return None
            self.queue.remove(member)
            self.active[member["name"]] = request
            return member, request

    def slot_loop(self, slot_index):
        while True:
            taken = self.next_member(slot_index)
            if taken is None:
                return
            if taken == "wait":
                time.sleep(self.poll)
                continue
            member, request = taken
            path = self.root / "requests" / f"{member['name']}.json"
            started = time.monotonic()
            try:
                write(path, request)
                self.log(event="start", member=member["name"], slot=slot_index, member_request=member["request"],
                         request=pin(path))
                supervisor = self.start_("dispatch", path, **self.host)
                # The wall-time limit bounds the native child; fingerprinting and the
                # archive follow it, so the wait allows as long again plus an hour.
                limit = dispatch.positive(request["wall_seconds"], "wall-time limit")
                report = self.wait_(request, supervisor, 2 * limit + 3600)
                self.results[member["name"]] = {"member_request": member["request"], "request": pin(path),
                                                "report": pin(report), "slot": slot_index}
                self.log(event="end", member=member["name"], slot=slot_index, report=pin(report),
                         seconds=time.monotonic() - started)
                if self.plan.get("post"):
                    self.posts.put((member, request, report))
            except BaseException as error:  # stop new starts; running members finish
                self.fail(f"{member['name']}: {error}")
                self.log(event="failed", member=member["name"], slot=slot_index, error=str(error),
                         seconds=time.monotonic() - started)
            finally:
                with self.lock:
                    self.active.pop(member["name"], None)

    def post_loop(self, cpu):
        while (item := self.posts.get()) is not None:
            member, request, report = item
            try:
                # Only these exact words are replaced; other text, braces included, stays.
                values = {"{name}": member["name"], "{report}": str(report), "{request_root}": request["root"]}
                command = []
                for part in self.plan["post"]:
                    for word, value in values.items():
                        part = part.replace(word, value)
                    command.append(part)
                self.log(event="post-start", member=member["name"], cpu=cpu, command=command)
                code = self.post_(cpu, command, self.plan["post_timeout_seconds"])
            except BaseException as error:
                code = f"{type(error).__name__}: {error}"
            self.log(event="post-end", member=member["name"], exit_code=code)
            if code != 0:
                self.fail(f"{member['name']} post command: {code}")

    def run(self, slots_):
        self.slots = slots_
        posts = [threading.Thread(target=self.post_loop, args=(cpu,)) for cpu in self.plan.get("post_cpus", [])]
        threads = [threading.Thread(target=self.slot_loop, args=(i,)) for i in range(len(slots_))]
        for thread in posts + threads:
            thread.start()
        try:
            for thread in threads:
                thread.join()
        except BaseException:  # an interrupt: running members finish, nothing new starts
            self.fail("interrupted")
            raise
        finally:
            for _ in posts:
                self.posts.put(None)
            for thread in posts:
                thread.join()
        not_started = [member["name"] for member in self.queue]
        summary = {"complete": not self.failures and not not_started, "failed": self.failed,
                   "failures": self.failures, "not_started": not_started, "members": self.results}
        write(self.root / "summary.json", summary)
        return summary


def member_selection(request):
    """The verified selection of a member's choice, and its pack's slots."""
    selected = read(checked(request["choice_verification"]))["selected"]
    require("pack" in selected, "a member's verified choice must select a pack")
    return selected, read(checked(selected["pack"]))["slots"]


def run(plan_path, start_=start, wait_=wait, admit_=admit, post_=run_post, storage_=None, poll=POLL_SECONDS):
    plan = read(plan_path)
    require(plan["schema"] == RUN and "slots" not in plan, "unsupported pack run plan")
    host_arguments(plan)
    names = [member["name"] for member in plan["members"]]
    require(names and len(set(names)) == len(names) and all(NAME.fullmatch(name) for name in names),
            "invalid member names")
    pack_slots, roots, outputs, families = None, set(), set(), {}
    for member in plan["members"]:
        request = read(checked(member["request"]))
        require(request["schema"] == dispatch.SCHEMA and request["kind"] == "training"
                and "choice" in request and "choice_verification" in request and "start_barrier" not in request,
                "members are production training requests with a verified choice")
        selected, own = member_selection(request)
        pack_slots = own if pack_slots is None else pack_slots
        # A study's arms run in one layout (the launcher's rule), here with equal slots.
        require(sorted(map(json.dumps, own)) == sorted(map(json.dumps, pack_slots)),
                "members' packs must have the same slots")
        require(request["placement"] in pack_slots, "a member's placement must be one of its pack's slots")
        found = {str(Path(request[key]).resolve()) for key in ("root", "cold_root")}
        require(not found & roots, "members need their own roots")
        roots |= found
        output = str(Path(read(checked(request["config"]))["output_directory"]).resolve())
        require(output not in outputs, "members need their own output directories")
        outputs.add(output)
        runs = families.setdefault(selected["workload"], [selected["study_runs"], 0])
        require(runs[0] == selected["study_runs"], "a family's members must cite one study size")
        runs[1] += 1
        require(runs[1] <= runs[0], "more members than the study's runs")
    require(len({shape(slot) for slot in pack_slots}) == 1, "a pack's slots must share one worker count")
    post, post_cpus = plan.get("post"), plan.get("post_cpus", [])
    if post is None:
        require(not post_cpus and "post_timeout_seconds" not in plan,
                "post CPUs and a post timeout belong to a post command")
    else:
        require(isinstance(post, list) and post and all(type(part) is str for part in post)
                and post_cpus and all(type(cpu) is int and cpu >= 0 for cpu in post_cpus)
                and len(set(post_cpus)) == len(post_cpus), "a post command needs its post CPUs")
        dispatch.positive(plan.get("post_timeout_seconds"), "post command timeout")
    require(isinstance(plan["stop_files"], list) and all(Path(path).is_absolute() for path in plan["stop_files"]),
            "absolute STOP file paths required")
    admit_(pack_slots, post_cpus)
    root = absolute(plan["root"])
    root.mkdir(parents=True)
    (root / "requests").mkdir()
    summary = Runner(plan, root, start_, wait_, post_, storage_, poll).run(pack_slots)
    print(json.dumps(summary))
    return summary


def post_main(argv):
    """_post CPU TIMEOUT -- CMD...: CMD and every process it starts on one CPU at
    below-normal priority, all stopped after TIMEOUT seconds; any still running
    when CMD exits are stopped then."""
    cpu, timeout, separator, *command = argv
    require(separator == "--" and command, "usage: _post CPU TIMEOUT -- CMD...")
    cpu, timeout = int(cpu), float(timeout)
    code = post_windows(cpu, timeout, command) if os.name == "nt" else post_linux(cpu, timeout, command)
    if code is None:
        print(f"post command stopped after {timeout} s", file=sys.stderr)
        return POST_TIMEOUT_CODE
    return code


def post_linux(cpu, timeout, command):
    """The command in its own process group; the group is stopped at the timeout
    or once the command exits. Returns its exit code, None when stopped."""
    dispatch.set_affinity([cpu])  # read back; every process of the command inherits it
    child = subprocess.Popen(command, start_new_session=True, preexec_fn=lambda: os.nice(10))
    deadline, stopped = time.monotonic() + timeout, False
    # Wait without reaping: an unreaped leader keeps its group's id from reuse.
    while os.waitid(os.P_PID, child.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
        if time.monotonic() > deadline:
            stopped = True
            break
        time.sleep(POST_POLL_SECONDS)
    try:
        os.killpg(child.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    code = child.wait()
    return None if stopped else code


def post_windows(cpu, timeout, command):
    """The command started suspended, then put in its own one-CPU below-normal job
    nested in the declaration's (whose affinity limit stops a process from setting
    its own) and its affinity read back before it runs. The job is stopped at the
    timeout and closed once the command exits, which stops what is left in it.
    Returns the command's exit code, None when stopped."""
    child = subprocess.Popen(command, creationflags=subprocess.BELOW_NORMAL_PRIORITY_CLASS | CREATE_SUSPENDED)
    tree = None
    try:
        tree = slots.Tree([cpu], "below_normal", contain_self=False)
        tree.adopt(child.pid, slots.creation_time(child.pid))  # refuses any other affinity
        require(slots._NtResumeProcess(int(child._handle)) >= 0, "cannot resume the post command")
        try:
            return child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            tree.kill()
            child.wait()
            return None
    except BaseException:
        if tree is not None:
            tree.kill()
        if child.poll() is None:
            child.kill()
        child.wait()
        raise
    finally:
        if tree is not None:
            slots._CloseHandle(tree.job)


def main(argv=None):
    require(not sys.flags.optimize, "optimized execution bypasses checks")
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == ["_post"]:
        return post_main(argv[1:])
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("action", choices=("qualify", "run"))
    parser.add_argument("plan", type=Path)
    args = parser.parse_args(argv)
    plan = absolute(args.plan)
    if args.action == "qualify":
        return 0 if qualify(plan)["outputs_identical"] else 1
    return 0 if run(plan)["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
