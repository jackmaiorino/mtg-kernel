"""W5 guarded Linux launcher (collab LANES/lead/W5-DESIGN.md v1.3, sections 2.1, 2.2 and 10 row E10).

Every W5 chain runs in an identical affinity box: a cpuset of whole physical cores with both SMT siblings. A box is
a cgroup v2 directory under the W5 root; the chain and the frozen filler run in separate child cgroups of it:

    <cgroup_root>                cpuset.cpus = every box CPU, cpuset.cpus.partition = root (exclusive CPUs)
    <cgroup_root>/box<k>         cpuset.cpus = the box's CPUs
    <cgroup_root>/box<k>/chain   the chain instance: memory.max, memory.swap.max = 0, memory.oom.group = 1
    <cgroup_root>/box<k>/filler  the frozen filler while the chain is paused

**Chain-only ledger.** A chain's CPU is its own cgroup's `cpu.stat usage_usec`, which sums every process of the
chain and never the filler. One cgroup instance lives from a chain process start to its exit; a pause freezes it
and leaves it alive. The ledger (`<state>/<chain>/ledger.jsonl`, append-only, each line carrying the SHA-256 of the
previous line) records every instance's final value, so

    ledger(t) = closed total of earlier instances + usage_usec of the live instance

across pauses, filler runs, resumes, restarts and the stage transition.

**Trainer interface** (the E2 hook writes the ledger value into the chain's receipt at every update boundary):
- `W5_LEDGER_CGROUP`: the chain cgroup directory; read `usage_usec` from its `cpu.stat`.
- `W5_LEDGER_BASE_USEC`: the closed total of earlier instances (an integer, constant for the instance).
- ledger value = `W5_LEDGER_BASE_USEC` + `usage_usec`; `W5_LEDGER_INSTANCE` numbers the instance.
- `W5_CONTROL_DIR`: the pause handshake, namespaced by instance. A pause happens only at an update boundary
  the trainer chooses: by its own rule (ledger past 1.10 R-hat) or because `pause.intent` exists (an operator's
  `w5box.py pause`). The trainer then writes `pause.request` (JSON `{"instance": W5_LEDGER_INSTANCE, "seq": n,
  ...}`, written to a temporary name and renamed) and only polls for `resume.<instance>.<n>.json`. The launcher
  freezes the chain cgroup, records the frozen value and starts the filler. `resume` freezes and kills the
  filler, writes `resume.<instance>.<n>.json` (with the operator's payload, e.g. R_i) and thaws the chain.
- `W5_BOX_SLOT`, `W5_BOX_CPUS`, `W5_HOST_CLASS`, `W5_CHAIN`, `W5_MANIFEST`: identity for receipts.
- `<state_root>/<chain>/ledger-source.json` (`W5_LEDGER_SOURCE`; `{ledger_source}` in an argv template): the E2
  trainer's `chain_ledger.source` (`w5-chain-ledger-source/v1`: `closed_usec`, `instance`, `cpu_stat`, and
  `stop_at_usec` once a resume names R_i), rewritten durably at instance open and before a resume thaws.
- A controller pause (`w5box.py pause --now`, the 1.10 R-hat rule run outside the trainer) freezes the chain at
  once: a frozen cgroup is transparent to its processes, and the trainer stops at `stop_at_usec` after the resume.
- The chain and filler environments are a fixed base (`PATH`, `LANG`) plus the plan's `env`, never the
  supervisor's, so the throughput binding covers them.

**Recovery.** A supervisor that dies leaves its instance open. The next `run` closes it exactly from the
surviving cgroup (with its OOM counters). If the cgroup is gone (a reboot), the ledger records the larger of the
last durable live reading and the pause value as a lower bound, and `run` refuses to continue that chain until
an operator accepts the gap (`--accept-lower-bound REASON`), the chain-failure decision being the lead's.

**Occupancy (amendment A1's default).** A paused chain's box must hold the frozen filler. The node log
(`<state_root>/occupancy.jsonl`, hash-chained) carries every outage (from the filler's exit to a verified
replacement), fault, refill, escalation and operator freeze or thaw, with the chains affected; all appends and all
enforcement run under one node lock. The moment a filler exits the outage opens and the node is not clear: every
measured chain cgroup on the node is frozen, by whichever supervisor or operator command enforces next, so it does
not depend on any one supervisor being alive. Restarts (up to FILLER_RESTARTS) close the outage only once verified
(alive, the process in the box's filler cgroup and cpuset, burning CPU); otherwise the box faults. `w5box.py refill`
empties the filler cgroup, starts and verifies a new filler, and only then records the restoration. Unpaused chains
thaw when no outage, fault, operator freeze or torn log tail remains. A new chain instance does not start while the
node is not clear. Escalation (`ESCALATE.json`): any outage open for 2 hours, or a fault at once in Stage 1b. Each
chain copies every node event into its ledger exactly once (a durable watermark).

**Held boxes.** When an instance of a chain whose spec says `handoff: true` (every chain the W5 controller,
tools/w5/chains.py, plans) closes for any reason, or any instance closes after an OOM kill, its supervisor opens the
box's outage before the box is released, so the node's other measured chains freeze at once; the chain's restart in
the box (`run` restores its own slot) or a verified filler (`w5box.py fill`, below) closes it. A box whose chain
has ended holds the frozen filler while the node's other measured chains run (`w5box.py fill`, started by
tools/w5/chains.py supervise), under the same rules: the box is an outage from the
command's start (`--empty-since` records when the chain closed), opened before it waits for the box lock, until a
verified filler runs in its filler cgroup; a filler that exits opens a new outage at once and is replaced, and
FILLER_RESTARTS failed replacements fault the box. Any other exit (SIGTERM, an error) opens the box's outage before
the filler goes, so an operator's restart retries with the node held. The `--stop` file (written once every
measured chain has ended; never removed here, so a late start exits at once) ends it: the filler goes and the box's
open outage closes.

**Recover.** `w5box.py recover` closes the open instance of a chain whose supervisor has died (nothing holds its
supervisor lock), as the next `run` would, without starting another: a chain still running there has no reader for
its pause, resume and kill requests, so its cgroup is killed first, and a `handoff` chain's box becomes an outage
before it is released.

**Guard (COMPUTE-POLICY item 5).** `run` refuses before any spawn unless the plan's pins hold (host class, the
complete detected box layout, binary and config hashes, `argv[0]` is the pinned binary and the config is in
`argv`) and a box throughput record binds this plan's complete launch specification (`w5-box-throughput/v1`:
serial and full-node concurrent phases with identical outputs). A qualification run (Stage 0) is labelled
(`--qualification LABEL --max-wall-seconds S`); without a record its wall cap is 6 hours (small correctness and
scaling checks only).

    w5box.py layout [--cores-per-box 2] [--offbox-cores-per-domain 2] [--out LAYOUT.json]
    w5box.py host-class
    w5box.py run --plan PLAN.json --chain CHAIN [--qualification LABEL --max-wall-seconds S]
    w5box.py pause|resume|kill|refill --plan PLAN.json --chain CHAIN [--payload JSON]
    w5box.py fill --plan PLAN.json --slot K --stop FILE [--empty-since UNIX]   an ended box holds the filler
    w5box.py recover --plan PLAN.json --chain CHAIN     close a dead supervisor's instance (stopping it if it runs)
    w5box.py node-freeze|node-thaw --plan PLAN.json --reason TEXT    operator freeze or thaw of the node's chains
    w5box.py occupancy --plan PLAN.json                           open faults, freeze state and outages
    w5box.py ledger --plan PLAN.json --chain CHAIN       current ledger value (closed + live)
    w5box.py verify-ledger --plan PLAN.json --chain CHAIN

Linux with cgroup v2 only; `run` needs write access to the cgroup root (root, or a delegated subtree).
"""
from __future__ import annotations

import argparse
import contextlib
import hashlib
import json
import os
import platform
import re
import signal
import sys
import time
from pathlib import Path

LAYOUT_SCHEMA = "w5-box-layout/v1"
PLAN_SCHEMA = "w5-launch-plan/v1"
THROUGHPUT_SCHEMA = "w5-box-throughput/v1"
LEDGER_SCHEMA = "w5-chain-ledger/v1"
MANIFEST_SCHEMA = "w5-chain-manifest/v1"
CONTROLLERS = ("cpuset", "cpu", "memory", "pids")
SYS_CPU = Path("/sys/devices/system/cpu")
POLL_SECONDS = 0.2
LIVE_EVERY_SECONDS = 5.0
SETTLE_SECONDS = 60.0
FREEZE_SECONDS = 30.0
QUALIFICATION_MAX_WALL = 8 * 86400
UNQUALIFIED_MAX_WALL = 6 * 3600
BASE_ENV = {"PATH": "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin", "LANG": "C.UTF-8"}
LEDGER_SOURCE_SCHEMA = "w5-chain-ledger-source/v1"
FILLER_RESTARTS = 3
ESCALATE_SECONDS = 2 * 3600
FILL_POLL_SECONDS = 1.0
REFILL_VERIFY_SECONDS = 10.0
REFILL_MIN_CPU_USEC = 1_000_000
OCCUPANCY_SCHEMA = "w5-node-occupancy/v1"
EXIT_OOM = 3
EXIT_REFUSED = 2
EXIT_CLEANUP = 4


class Refusal(Exception):
    """A launch refusal: raised before any spawn."""


def require(ok: bool, message: str) -> None:
    if not ok:
        raise Refusal(message)


# ---------------------------------------------------------------- hashing and files

def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def canonical(value) -> bytes:
    return json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")


def write_json_atomic(path: Path, value, durable: bool = False) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(f"{path.name}.{os.getpid()}.{time.time_ns()}.tmp")
    with open(tmp, "w", encoding="utf-8", newline="\n") as handle:
        handle.write(json.dumps(value, indent=2, sort_keys=True) + "\n")
        if durable:
            handle.flush()
            os.fsync(handle.fileno())
    os.replace(tmp, path)
    if durable and hasattr(os, "O_DIRECTORY"):
        fd = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)


# ---------------------------------------------------------------- cpu lists and topology

def parse_cpu_list(text: str) -> list[int]:
    cpus: list[int] = []
    for part in text.strip().split(","):
        if not part:
            continue
        if "-" in part:
            lo, hi = part.split("-")
            cpus.extend(range(int(lo), int(hi) + 1))
        else:
            cpus.append(int(part))
    return sorted(set(cpus))


def format_cpu_list(cpus) -> str:
    cpus = sorted(set(cpus))
    runs, start = [], None
    for i, cpu in enumerate(cpus):
        if start is None:
            start = cpu
        if i + 1 == len(cpus) or cpus[i + 1] != cpu + 1:
            runs.append(str(start) if start == cpu else f"{start}-{cpu}")
            start = None
    return ",".join(runs)


def read_topology(sys_cpu: Path = SYS_CPU) -> dict:
    """Physical cores (SMT sibling sets) grouped by L3 domain (the package when no L3 is listed)."""
    online = parse_cpu_list((sys_cpu / "online").read_text())
    cores, domains = {}, {}
    for cpu in online:
        topo = sys_cpu / f"cpu{cpu}" / "topology"
        sib = topo / "core_cpus_list"
        siblings = tuple(parse_cpu_list((sib if sib.exists() else topo / "thread_siblings_list").read_text()))
        l3 = sys_cpu / f"cpu{cpu}" / "cache" / "index3" / "shared_cpu_list"
        if l3.exists():
            domain = tuple(parse_cpu_list(l3.read_text()))
        else:
            pkg = topo / "package_cpus_list"
            domain = tuple(parse_cpu_list((pkg if pkg.exists() else topo / "core_siblings_list").read_text()))
        cores[siblings] = domain
        domains.setdefault(domain, set()).add(siblings)
    ordered = []
    for domain in sorted(domains, key=min):
        ordered.append([list(core) for core in sorted(domains[domain], key=min)])
    return {"online": online, "domains": ordered}


def compute_layout(topology: dict, cores_per_box: int, offbox_cores_per_domain: int,
                   boxes: int | None = None) -> dict:
    """Boxes of whole cores inside one L3 domain each; the first cores of every domain stay off the boxes."""
    require(cores_per_box >= 1 and offbox_cores_per_domain >= 0, "Bad box shape")
    slots, offbox = [], []
    for domain in topology["domains"]:
        offbox += [cpu for core in domain[:offbox_cores_per_domain] for cpu in core]
        rest = domain[offbox_cores_per_domain:]
        for i in range(len(rest) // cores_per_box):
            group = rest[i * cores_per_box:(i + 1) * cores_per_box]
            slots.append({"cores": group, "cpus": sorted(cpu for core in group for cpu in core)})
        offbox += [cpu for core in rest[(len(rest) // cores_per_box) * cores_per_box:] for cpu in core]
    if boxes is not None:
        require(0 < boxes <= len(slots), f"Only {len(slots)} boxes fit this host")
        for extra in slots[boxes:]:
            offbox += extra["cpus"]
        slots = slots[:boxes]
    require(slots, "No box fits this host")
    threads = {len(core) for slot in slots for core in slot["cores"]}
    require(len(threads) == 1, "Boxes would mix SMT and non-SMT cores")
    for index, slot in enumerate(slots):
        slot["slot"] = index
    positions = {"boxes": [slot["cpus"] for slot in slots], "offbox": sorted(offbox)}
    return {"schema": LAYOUT_SCHEMA, "cores_per_box": cores_per_box,
            "offbox_cores_per_domain": offbox_cores_per_domain, "threads_per_core": threads.pop(),
            "boxes": slots, "offbox_cpus": sorted(offbox), "positions_sha256": sha256_bytes(canonical(positions))}


def host_class(cpuinfo: str | None = None, online: list[int] | None = None) -> str:
    """`linux-zen5-9950x` on the W5 nodes; any other host gets a class named after its CPU model."""
    if cpuinfo is None:
        cpuinfo = Path("/proc/cpuinfo").read_text(errors="replace")
    if online is None:
        online = parse_cpu_list((SYS_CPU / "online").read_text())
    match = re.search(r"^model name\s*:\s*(.+)$", cpuinfo, re.MULTILINE)
    model = match.group(1).strip() if match else "unknown"
    if "Ryzen 9 9950X" in model and "9950X3D" not in model and len(online) == 32:
        return "linux-zen5-9950x"
    return "linux-" + re.sub(r"[^a-z0-9]+", "-", model.lower()).strip("-")


# ---------------------------------------------------------------- cgroup v2

class Cgroup:
    def __init__(self, path):
        self.path = Path(path)

    def __repr__(self):
        return f"Cgroup({self.path})"

    def read(self, name: str) -> str:
        return (self.path / name).read_text()

    def write(self, name: str, value) -> None:
        with open(self.path / name, "w") as handle:
            handle.write(str(value))

    def keyed(self, name: str) -> dict[str, int]:
        out = {}
        for line in self.read(name).splitlines():
            if line.strip():
                key, value = line.split()[:2]
                out[key] = int(value) if value.lstrip("-").isdigit() else value
        return out

    def exists(self) -> bool:
        return (self.path / "cgroup.procs").exists()

    def usage_usec(self) -> int:
        return int(self.keyed("cpu.stat")["usage_usec"])

    def populated(self) -> bool:
        return self.keyed("cgroup.events").get("populated") == 1

    def frozen(self) -> bool:
        return self.keyed("cgroup.events").get("frozen") == 1

    def inode(self) -> int:
        return os.stat(self.path).st_ino

    def enable(self, controllers=CONTROLLERS) -> None:
        have = set(self.read("cgroup.subtree_control").split())
        missing = [c for c in controllers if c not in have]
        if missing:
            available = set(self.read("cgroup.controllers").split())
            require(set(missing) <= available, f"{self.path}: controllers {missing} not available")
            self.write("cgroup.subtree_control", " ".join("+" + c for c in missing))

    def freeze(self, on: bool, timeout: float = FREEZE_SECONDS) -> None:
        self.write("cgroup.freeze", 1 if on else 0)
        deadline = time.monotonic() + timeout
        while self.populated() and self.frozen() != on:
            if time.monotonic() > deadline:
                raise RuntimeError(f"{self.path}: freeze={on} did not settle in {timeout} s")
            time.sleep(0.01)

    def kill(self) -> None:
        self.write("cgroup.kill", 1)

    def wait_empty(self, timeout: float) -> bool:
        deadline = time.monotonic() + timeout
        while self.populated():
            if time.monotonic() > deadline:
                return False
            time.sleep(0.05)
        return True

    def rmdir(self) -> None:
        os.rmdir(self.path)


def prepare_root(root: Cgroup, layout: dict, isolation: str) -> None:
    """The W5 root: every box CPU, exclusive (a cpuset partition root) unless isolation is `none`."""
    parent = Cgroup(root.path.parent)
    require(parent.exists(), f"{parent.path} is not a cgroup v2 directory")
    parent.enable()
    cpus = sorted(cpu for slot in layout["boxes"] for cpu in slot["cpus"])
    if not root.exists():
        root.path.mkdir()
        root.write("cpuset.cpus", format_cpu_list(cpus))
        root.write("cpuset.mems", parent.read("cpuset.mems.effective").strip())
        if isolation == "partition-root":
            root.write("cpuset.cpus.partition", "root")
    require(parse_cpu_list(root.read("cpuset.cpus")) == cpus, f"{root.path} holds a different box set")
    if isolation == "partition-root":
        state = root.read("cpuset.cpus.partition").strip()
        require(state == "root", f"{root.path}: cpuset partition is '{state}', not an exclusive root")
    root.enable()


def prepare_box(root: Cgroup, slot: dict) -> Cgroup:
    box = Cgroup(root.path / f"box{slot['slot']}")
    if not box.exists():
        box.path.mkdir()
        box.write("cpuset.cpus", format_cpu_list(slot["cpus"]))
        box.write("cpuset.mems", root.read("cpuset.mems.effective").strip())
    require(parse_cpu_list(box.read("cpuset.cpus.effective")) == slot["cpus"],
            f"{box.path}: effective CPUs differ from the layout")
    box.enable()
    return box


def make_leaf(box: Cgroup, name: str, memory_max: int) -> Cgroup:
    leaf = Cgroup(box.path / name)
    require(not leaf.exists(), f"{leaf.path} already exists (an instance is live or unrecovered)")
    leaf.path.mkdir()
    leaf.write("memory.max", int(memory_max))
    if (leaf.path / "memory.swap.max").exists():
        leaf.write("memory.swap.max", 0)
    leaf.write("memory.oom.group", 1)
    return leaf


def spawn_into(cgroup: Cgroup, argv: list[str], env: dict, log_path: Path, cwd: str | None = None,
               frozen: bool = False) -> int:
    """Fork, move the child into the cgroup before exec, so every descendant is metered there. With `frozen`
    the cgroup is frozen first: the child stops as it joins (before exec) and runs only when the cgroup thaws;
    the parent then confirms membership from cgroup.procs instead of the child's acknowledgement."""
    if frozen:
        cgroup.write("cgroup.freeze", 1)
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_fd = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
    procs = str(cgroup.path / "cgroup.procs")
    ack_r, ack_w = os.pipe()  # the child acknowledges joining the cgroup before exec
    pid = os.fork()
    if pid == 0:  # child: only async-signal-safe-ish work, then exec
        try:
            os.close(ack_r)
            # Close every inherited descriptor (the node log's flock included) before joining: a frozen child
            # must not hold a lock that the thawing enforcement needs.
            for name in os.listdir("/proc/self/fd"):
                fd = int(name)
                if fd > 2 and fd not in (ack_w, log_fd):
                    try:
                        os.close(fd)
                    except OSError:
                        pass
            with open(procs, "w") as handle:
                handle.write(str(os.getpid()))
            try:
                os.write(ack_w, b"1")
            except OSError:  # a frozen spawn's parent may have stopped listening
                pass
            os.close(ack_w)
            signal.signal(signal.SIGPIPE, signal.SIG_DFL)  # the chain gets the default disposition, not Python's
            os.setsid()
            os.dup2(log_fd, 1)
            os.dup2(log_fd, 2)
            null = os.open(os.devnull, os.O_RDONLY)
            os.dup2(null, 0)
            if cwd:
                os.chdir(cwd)
            os.execvpe(argv[0], argv, env)
        except BaseException as exc:  # noqa: BLE001
            os.write(2, f"w5box: spawn failed: {exc}\n".encode())
        os._exit(127)
    os.close(log_fd)
    os.close(ack_w)
    if frozen:
        deadline = time.monotonic() + SETTLE_SECONDS
        while pid not in {int(x) for x in cgroup.read("cgroup.procs").split()}:
            done, _ = os.waitpid(pid, os.WNOHANG)
            if done == pid or time.monotonic() > deadline:
                os.close(ack_r)
                raise RuntimeError(f"frozen spawn into {cgroup.path} failed before the child joined")
            time.sleep(0.01)
        os.close(ack_r)
        return pid
    joined = os.read(ack_r, 1)
    os.close(ack_r)
    if joined != b"1":  # the child died before joining: nothing ran outside the meter's knowledge
        os.waitpid(pid, 0)
        raise RuntimeError(f"spawn into {cgroup.path} failed before the child joined")
    return pid


def verify_filler(cg: Cgroup, pid: int, cpus: list[int]) -> tuple[dict, int | None]:
    """A new filler is alive, its process is in the box's filler cgroup and cpuset, and it burns CPU. Returns the
    checks and the pid (None once it has exited and been reaped)."""
    before = cg.usage_usec()
    deadline = time.monotonic() + REFILL_VERIFY_SECONDS
    while time.monotonic() < deadline:
        time.sleep(0.25)
        done, _ = os.waitpid(pid, os.WNOHANG)
        if done == pid:
            pid = None
            break
    procs = {int(x) for x in cg.read("cgroup.procs").split()}
    burned = cg.usage_usec() - before
    checks = {"alive": pid is not None, "pid_in_filler_cgroup": pid in procs,
              "cpuset": parse_cpu_list(cg.read("cpuset.cpus.effective")) == cpus,
              "burning": burned >= REFILL_MIN_CPU_USEC}
    return dict(checks, ok=all(checks.values()), burned_usec=burned), pid


def rusage_usec(ru) -> int:
    return int(round((ru.ru_utime + ru.ru_stime) * 1e6))


# ---------------------------------------------------------------- the ledger

class Ledger:
    """Append-only chain ledger; every line names the SHA-256 of the previous line."""

    def __init__(self, path: Path):
        self.path = Path(path)

    def records(self) -> list[dict]:
        if not self.path.exists():
            return []
        out, prev = [], None
        for number, raw in enumerate(self.path.read_bytes().splitlines(), 1):
            record = json.loads(raw)
            if record.get("prev_sha256") != prev:
                raise ValueError(f"{self.path}: line {number} breaks the hash chain")
            prev = sha256_bytes(raw)
            out.append(record)
        return out

    def append(self, record: dict) -> dict:
        records = self.records()
        prev = sha256_bytes(self.path.read_bytes().splitlines()[-1]) if records else None
        record = dict(record, schema=LEDGER_SCHEMA, prev_sha256=prev, unix=round(time.time(), 3))
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with open(self.path, "ab") as handle:
            handle.write(canonical(record) + b"\n")
            handle.flush()
            os.fsync(handle.fileno())
        return record

    def summary(self) -> dict:
        """Closed total, the open instance (if any) and the next instance number."""
        closed, open_instance, instances, unresolved = 0, None, 0, []
        for record in self.records():
            if record["event"] == "close" and record.get("lower_bound"):
                unresolved.append(record["instance"])
            elif record["event"] == "accept-lower-bound":
                unresolved = [i for i in unresolved if i != record["instance"]]
            if record["event"] == "open":
                require(open_instance is None, "Ledger opens an instance while another is open")
                require(record["base_usec"] == closed, "Ledger base differs from the closed total")
                open_instance = record
                instances = record["instance"] + 1
            elif record["event"] == "close":
                require(open_instance is not None and record["instance"] == open_instance["instance"],
                        "Ledger closes an instance that is not open")
                closed += record["final_usec"]
                require(record["closed_total_usec"] == closed, "Ledger closed total does not add up")
                open_instance = None
        return {"closed_total_usec": closed, "open": open_instance, "next_instance": instances,
                "unresolved_lower_bounds": unresolved}


def ledger_value(ledger: Ledger, chain_cgroup: Cgroup) -> dict:
    summary = ledger.summary()
    live = None
    if summary["open"] is not None and chain_cgroup.exists():
        live = chain_cgroup.usage_usec()
    return {"closed_total_usec": summary["closed_total_usec"], "live_usec": live,
            "ledger_usec": summary["closed_total_usec"] + (live or 0), "open": summary["open"] is not None}


# ---------------------------------------------------------------- plan and guard

def load_plan(path) -> tuple[dict, str]:
    raw = Path(path).read_bytes()
    plan = json.loads(raw)
    require(plan.get("schema") == PLAN_SCHEMA, "Wrong plan schema")
    for key in ("host_class", "layout", "cgroup_root", "state_root", "isolation", "chains", "filler"):
        require(key in plan, f"Plan lacks {key}")
    require(plan["isolation"] in ("partition-root", "none"), "Isolation must be partition-root or none")
    for key in ("cgroup_root", "state_root"):
        require(Path(plan[key]).is_absolute(), f"Plan {key} must be an absolute path")
    return plan, sha256_bytes(raw)


def chain_spec(plan: dict, chain: str) -> dict:
    spec = plan["chains"].get(chain)
    require(spec is not None, f"Plan has no chain {chain}")
    for key in ("box_slot", "arm", "argv", "binary", "config", "memory_max_bytes", "seed_root"):
        require(key in spec, f"Chain {chain} lacks {key}")
    return spec


LAYOUT_KEYS = ("schema", "cores_per_box", "offbox_cores_per_domain", "threads_per_core", "boxes", "offbox_cpus",
               "positions_sha256")


def chain_argv(spec: dict, chain: str, state: Path) -> list[str]:
    """The chain's argv from its arm-level template (`{chain}`, `{seed_root}`, `{state}`, `{ledger_source}`)."""
    return [a.format(chain=chain, seed_root=spec["seed_root"], state=str(state),
                     ledger_source=str(Path(state) / "ledger-source.json")) for a in spec["argv"]]


def filler_argv(filler: dict, seed_root: str, slot: int, seq: int) -> list[str]:
    return [a.format(seed_root=seed_root, slot=slot, seq=seq) for a in filler["argv"]]


def check_launch(item: dict, argv: list[str], what: str) -> None:
    """The pinned binary is what runs and the pinned config is what it reads."""
    for key in ("binary", "config"):
        require(sha256_file(item[key]["path"]) == item[key]["sha256"], f"{what} {key} hash changed: "
                f"{item[key]['path']}")
    require(argv and os.path.realpath(argv[0]) == os.path.realpath(item["binary"]["path"]),
            f"{what} argv[0] is not the pinned binary")
    require(any(a == item["config"]["path"] or a.endswith("=" + item["config"]["path"]) for a in argv[1:]),
            f"{what} argv does not name the pinned config")
    require(int(item["memory_max_bytes"]) > 0, f"{what} memory limit required")


def check_pins(plan: dict, chain: str, spec: dict, layout: dict, detected_class: str) -> None:
    require(detected_class == plan["host_class"],
            f"Host class {detected_class} differs from the plan's {plan['host_class']}")
    frozen = plan["layout"]
    require({k: frozen.get(k) for k in LAYOUT_KEYS} == {k: layout[k] for k in LAYOUT_KEYS},
            "The plan's box layout differs from the layout detected on this host")
    require(0 <= spec["box_slot"] < len(layout["boxes"]), "Chain box slot is not in the layout")
    check_launch(spec, chain_argv(spec, chain, Path(plan["state_root"]) / chain), "chain")
    filler = plan["filler"]
    for key in ("argv", "binary", "config", "memory_max_bytes", "seed_root_template"):
        require(key in filler, f"Filler lacks {key}")
    check_launch(filler, filler_argv(filler, filler["seed_root_template"].format(j=0), 0, 0), "filler")


def launch_spec_sha256(plan: dict, spec: dict) -> str:
    """Everything that defines the measured work in a box (arm-level: the argv template, not a chain's seeds)."""
    keys = ("argv", "cwd", "binary", "config", "memory_max_bytes", "arm")
    filler_keys = ("argv", "binary", "config", "memory_max_bytes", "seed_root_template")
    return sha256_bytes(canonical({"chain": {k: spec.get(k) for k in keys}, "env": plan.get("env", {}),
                                   "filler": {k: plan["filler"].get(k) for k in filler_keys},
                                   "isolation": plan["isolation"]}))


def throughput_binding(plan: dict, spec: dict) -> dict:
    return {"host_class": plan["host_class"], "positions_sha256": plan["layout"]["positions_sha256"],
            "cores_per_box": plan["layout"]["cores_per_box"], "boxes": len(plan["layout"]["boxes"]),
            "arm": spec["arm"], "binary_sha256": spec["binary"]["sha256"], "config_sha256": spec["config"]["sha256"],
            "launch_spec_sha256": launch_spec_sha256(plan, spec)}


def require_throughput(record: dict | None, binding: dict) -> dict:
    """COMPUTE-POLICY items 2 to 5 for the boxes: serial and full-node phases measured, identical outputs."""
    require(record is not None, "Box throughput record missing (COMPUTE-POLICY item 5)")
    require(record.get("schema") == THROUGHPUT_SCHEMA, "Wrong throughput record schema")
    for key, value in binding.items():
        require(record["binding"].get(key) == value, f"Throughput record binds a different {key}")
    phases = record["phases"]
    concurrent = [p["concurrent_boxes"] for p in phases]
    require(concurrent == sorted(set(concurrent)) and concurrent[0] == 1 and concurrent[-1] == binding["boxes"],
            "Throughput needs a single-box phase and a full-node phase, in increasing order")
    reference = None
    for phase in phases:
        require(len(phase["boxes"]) == phase["concurrent_boxes"], "Phase box count mismatch")
        for box in phase["boxes"]:
            require(box["seconds"] > 0 and box["updates"] > 0 and box["ledger_usec"] > 0, "Phase lacks work")
            rows = box["identity"]
            require(rows and isinstance(rows, dict), "Phase lacks identity rows")
            reference = rows if reference is None else reference
            require(rows == reference, "Concurrent output differs from the single-box reference")
    full = phases[-1]
    rate = sum(b["updates"] / b["seconds"] for b in full["boxes"])
    utilization = [b["ledger_usec"] / 1e6 / b["seconds"] for b in full["boxes"]]
    return {"full_node_updates_per_second": rate, "in_box_cpu_per_wall": utilization}


# ---------------------------------------------------------------- the supervisor


def flock(path: Path, what: str):
    import fcntl
    path.parent.mkdir(parents=True, exist_ok=True)
    handle = open(path, "w")
    try:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        handle.close()
        raise Refusal(f"{what} is held by another supervisor")
    return handle


class NodeLog:
    """The node's occupancy log (hash-chained JSON lines). Every append and every enforcement decision runs under
    one node lock, so a fault can never interleave with a thaw. A torn tail (a crash mid-append) freezes the node
    until the next append repairs it."""

    def __init__(self, state_root: Path):
        self.path = Path(state_root) / "occupancy.jsonl"
        self.lock_path = Path(state_root) / "occupancy.lock"

    @contextlib.contextmanager
    def locked(self):
        import fcntl
        self.lock_path.parent.mkdir(parents=True, exist_ok=True)
        with open(self.lock_path, "w") as handle:
            fcntl.flock(handle, fcntl.LOCK_EX)
            yield

    def read(self) -> tuple[list[dict], bool]:
        """Complete records (hash chain checked) and whether a torn tail follows them. Call under the lock."""
        if not self.path.exists():
            return [], False
        data = self.path.read_bytes()
        torn = bool(data) and not data.endswith(b"\n")
        lines = data.split(b"\n")[:-1] if data else []
        records, prev = [], None
        for number, raw in enumerate(lines, 1):
            record = json.loads(raw)
            if record.get("prev_sha256") != prev:
                raise ValueError(f"{self.path}: line {number} breaks the hash chain")
            prev = sha256_bytes(raw)
            records.append(record)
        return records, torn

    def append(self, record: dict) -> dict:
        """Append under the caller's lock; a torn tail is cut and recorded first."""
        data = self.path.read_bytes() if self.path.exists() else b""
        if data and not data.endswith(b"\n"):  # the cut log and its repair record land in one atomic rename
            cut = data.rfind(b"\n") + 1
            kept = data[:cut]
            lines = kept.split(b"\n")[:-1]
            repair = dict({"event": "torn-tail-repaired", "bytes": len(data) - cut}, log=OCCUPANCY_SCHEMA,
                          prev_sha256=sha256_bytes(lines[-1]) if lines else None, unix=round(time.time(), 3))
            tmp = self.path.with_name(f"{self.path.name}.{os.getpid()}.{time.time_ns()}.tmp")
            with open(tmp, "wb") as handle:
                handle.write(kept + canonical(repair) + b"\n")
                handle.flush()
                os.fsync(handle.fileno())
            os.replace(tmp, self.path)
        return self._write(record)

    def _write(self, record: dict) -> dict:
        data = self.path.read_bytes() if self.path.exists() else b""
        lines = data.split(b"\n")[:-1] if data else []
        prev = sha256_bytes(lines[-1]) if lines else None
        record = dict(record, log=OCCUPANCY_SCHEMA, prev_sha256=prev, unix=round(time.time(), 3))
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with open(self.path, "ab") as handle:
            handle.write(canonical(record) + b"\n")
            handle.flush()
            os.fsync(handle.fileno())
        return record

    @staticmethod
    def state(records: list[dict], torn: bool = False) -> dict:
        """Open outages and faults by box, escalations, the operator freeze, the last enforcement."""
        outages, faults, escalated, operator, enforced, torn_hold = {}, {}, set(), None, None, None
        for r in records:
            event = r["event"]
            if event == "torn-tail-repaired":  # the lost event may have been a fault: hold until an operator thaw
                torn_hold = r
            elif event == "outage-start":
                outages.setdefault(r["box_slot"], r)
            elif event == "fault":
                faults[r["box_slot"]] = r
            elif event == "restored":
                outages.pop(r["box_slot"], None)
                faults.pop(r["box_slot"], None)
            elif event == "escalate":
                escalated.add(r["outage_id"])
            elif event == "node-freeze":
                operator = r
            elif event == "node-thaw":
                operator = None
                torn_hold = None
            elif event == "enforced":
                enforced = r
        return {"open_outages": outages, "open_faults": faults, "escalated": sorted(escalated),
                "operator_freeze": operator, "torn_tail": torn, "torn_hold": torn_hold,
                "frozen": bool(outages) or bool(faults) or operator is not None or torn or torn_hold is not None,
                "enforced_frozen": None if enforced is None else enforced["frozen"]}


def node_chains(state_root: Path) -> list[dict]:
    """The chains with a live owner record on the node (their boxes)."""
    out = []
    for path in sorted(Path(state_root).glob("box*.owner.json")):
        try:
            out.append(json.loads(path.read_text()))
        except (OSError, ValueError):
            continue
    return out


def enforce_node(plan: dict, node: NodeLog, reason: str) -> dict:
    """Under the node lock: freeze every measured chain cgroup on the node while the node is not clear, thaw the
    unpaused ones when it is, and escalate overdue outages. Any supervisor or operator command may run it, so
    enforcement does not depend on a particular supervisor being alive."""
    state_root, cgroup_root = Path(plan["state_root"]), Path(plan["cgroup_root"])
    with node.locked():
        records, torn = node.read()
        state = NodeLog.state(records, torn)
        changed = []
        for box in sorted(cgroup_root.glob("box*")):
            chain_cg = Cgroup(box / "chain")
            if not chain_cg.exists() or not chain_cg.populated():
                continue
            owner_path = state_root / f"{box.name}.owner.json"
            owner = json.loads(owner_path.read_text()) if owner_path.exists() else None
            paused = owner is not None and (state_root / owner["chain"] / "paused.json").exists()
            want = state["frozen"] or paused or owner is None
            if chain_cg.frozen() != want:
                chain_cg.freeze(want)
                changed.append({"box": box.name, "chain": None if owner is None else owner["chain"], "frozen": want})
        if state["enforced_frozen"] != state["frozen"] or changed:
            node.append({"event": "enforced", "frozen": state["frozen"], "reason": reason, "changed": changed,
                         "open_outages": sorted(state["open_outages"]), "open_faults": sorted(state["open_faults"]),
                         "operator_freeze": state["operator_freeze"] is not None})
        for slot, outage in state["open_outages"].items():
            if outage["id"] in state["escalated"]:
                continue
            fault = state["open_faults"].get(slot)
            stage_1b = plan.get("stage") == "1b" and fault is not None
            if stage_1b or time.time() - outage["unix"] >= ESCALATE_SECONDS:
                escalation = {"event": "escalate", "box_slot": slot, "outage_id": outage["id"],
                              "outage_unix": outage["unix"], "stage": plan.get("stage"),
                              "deadline": "immediate (Stage 1b fault)" if stage_1b else "2 hours"}
                node.append(escalation)
                write_json_atomic(state_root / "ESCALATE.json", escalation, durable=True)
        return state


class Supervisor:
    def __init__(self, plan: dict, plan_sha: str, chain: str, qualification: str | None,
                 max_wall: float | None, layout: dict, detected_class: str, throughput: dict | None,
                 accept_lower_bound: str | None = None):
        self.plan, self.plan_sha, self.chain = plan, plan_sha, chain
        self.spec = chain_spec(plan, chain)
        self.layout = layout  # the detected layout, checked equal to the plan's
        self.slot = layout["boxes"][self.spec["box_slot"]]
        self.qualification, self.max_wall = qualification, max_wall
        self.detected_class, self.throughput = detected_class, throughput
        self.accept_lower_bound = accept_lower_bound
        self.state_root = Path(plan["state_root"])
        self.state = self.state_root / chain
        self.control = self.state / "control"
        self.ledger = Ledger(self.state / "ledger.jsonl")
        self.root = Cgroup(plan["cgroup_root"])
        self.box = Cgroup(self.root.path / f"box{self.slot['slot']}")
        self.chain_cg = Cgroup(self.box.path / "chain")
        self.filler_cg = Cgroup(self.box.path / "filler")
        self.owner_path = self.state_root / f"box{self.slot['slot']}.owner.json"
        self.ledger_source = self.state / "ledger-source.json"
        self.node = NodeLog(self.state_root)
        self.paused_path = self.state / "paused.json"
        self.filler_spawns = 0
        self.node_copied = None  # the next node index to copy, recovered from this chain's ledger
        self.instance = self.base = self.pid = self.filler_pid = None
        self.paused_seq = self.paused_usec = None

    # --- recovery of an instance whose supervisor died
    def recover(self, killed_running: bool = False) -> None:
        """Closes an instance whose supervisor died (`killed_running`: `w5box.py recover` stopped it while it still
        ran, so it ran until now)."""
        owner = json.loads(self.owner_path.read_text()) if self.owner_path.exists() else None
        if owner is not None and owner["chain"] != self.chain:
            other = Ledger(self.state_root / owner["chain"] / "ledger.jsonl").summary()
            require(other["open"] is None, f"Box {self.slot['slot']} holds chain {owner['chain']}'s open instance")
        summary = self.ledger.summary()
        record = summary["open"]
        if record is None:
            require(not self.chain_cg.exists() or not self.chain_cg.populated(),
                    f"{self.chain_cg.path} is populated but no ledger has an open instance there")
        else:
            instance_dir = self.state / f"instance-{record['instance']:04d}"
            if self.spec.get("handoff") is True:  # the dead instance's box is empty: held before anything else
                with self.node.locked():
                    records, torn = self.node.read()
                    if self.slot["slot"] not in NodeLog.state(records, torn)["open_outages"]:
                        self.node.append({"event": "outage-start", "box_slot": self.slot["slot"], "handoff": True,
                                          "id": f"{self.chain}/recover/handoff/{time.time_ns()}",
                                          "chain": self.chain, "affected": node_chains(self.state_root),
                                          "reason": "a dead supervisor's instance closes: its box waits to be "
                                                    "occupied"})
                enforce_node(self.plan, self.node, "recover")
            if self.chain_cg.exists() and self.chain_cg.inode() == record["cgroup_inode"]:
                require(not self.chain_cg.populated(), f"Chain {self.chain} instance is still running")
                final, lower, how = self.chain_cg.usage_usec(), False, "cgroup"
                events = self.chain_cg.keyed("memory.events")
                oom = int(events.get("oom_kill", 0)) + int(events.get("oom_group_kill", 0))
            else:
                live = instance_dir / "live.json"
                readings = [json.loads(live.read_text())["usage_usec"]] if live.exists() else []
                readings += [r["usage_usec"] for r in self.ledger.records()
                             if r["event"] in ("pause", "resume") and r["instance"] == record["instance"]]
                final, lower, how, oom = max(readings, default=0), True, "cgroup lost: last durable reading", None
            self.ledger.append({"event": "close", "instance": record["instance"], "final_usec": final,
                                "closed_total_usec": summary["closed_total_usec"] + final, "exit": None,
                                "oom_kill": oom, "chain_failure": None if oom is None else (bool(oom) or None),
                                "recovered": how, "lower_bound": lower, "killed_running": killed_running})
        if self.chain_cg.exists():
            self.chain_cg.rmdir()
        if self.filler_cg.exists():  # left by a dead supervisor: it held the box until this chain runs again
            with self.node.locked():
                self.node.append({"event": "outage-start", "id": f"{self.chain}/recovery/{time.time_ns()}",
                                  "box_slot": self.slot["slot"], "chain": self.chain, "reason": "recovery",
                                  "affected": node_chains(self.state_root)})
            enforce_node(self.plan, self.node, "recovery")
            self.filler_cg.kill()
            require(self.filler_cg.wait_empty(SETTLE_SECONDS), "A leftover filler did not die")
            self.filler_cg.rmdir()
        self.paused_path.unlink(missing_ok=True)
        if self.owner_path.exists():
            self.owner_path.unlink()

    def filler_seed_root(self) -> str:
        return self.plan["filler"]["seed_root_template"].format(j=self.slot["slot"])

    def manifest(self, argv: list[str]) -> dict:
        spec = self.spec
        return {"schema": MANIFEST_SCHEMA, "chain": self.chain, "arm": spec["arm"], "instance": self.instance,
                "base_usec": self.base, "host_class": self.detected_class, "hostname": platform.node(),
                "kernel": platform.release(), "box_slot": self.slot["slot"], "box_cpus": self.slot["cpus"],
                "box_cores": self.slot["cores"], "layout_positions_sha256": self.layout["positions_sha256"],
                "cgroup_root": str(self.root.path), "isolation": self.plan["isolation"],
                "memory_max_bytes": int(spec["memory_max_bytes"]), "argv": argv, "cwd": spec.get("cwd"),
                "binary": spec["binary"], "config": spec["config"], "seed_root": spec["seed_root"],
                "launch_spec_sha256": launch_spec_sha256(self.plan, spec),
                "filler": {"config_sha256": self.plan["filler"]["config"]["sha256"],
                           "binary_sha256": self.plan["filler"]["binary"]["sha256"],
                           "seed_root": self.filler_seed_root(),
                           "memory_max_bytes": int(self.plan["filler"]["memory_max_bytes"])},
                "plan_sha256": self.plan_sha, "launcher_sha256": sha256_file(__file__),
                "qualification": self.qualification, "max_wall_seconds": self.max_wall,
                "throughput": self.throughput, "accepted_lower_bound": self.accept_lower_bound,
                "started_unix": round(time.time(), 3)}

    def env(self, manifest_path: Path) -> dict:
        """Nothing inherited from the supervisor: a fixed base plus the plan's (bound) environment."""
        env = dict(BASE_ENV)
        env.update(self.plan.get("env", {}))
        env.update({"W5_LEDGER_CGROUP": str(self.chain_cg.path), "W5_LEDGER_BASE_USEC": str(self.base),
                    "W5_LEDGER_INSTANCE": str(self.instance), "W5_CONTROL_DIR": str(self.control),
                    "W5_BOX_SLOT": str(self.slot["slot"]), "W5_BOX_CPUS": format_cpu_list(self.slot["cpus"]),
                    "W5_HOST_CLASS": self.detected_class, "W5_CHAIN": self.chain,
                    "W5_MANIFEST": str(manifest_path), "W5_LEDGER_SOURCE": str(self.ledger_source)})
        return env

    def write_ledger_source(self, stop_at_usec: int | None) -> None:
        """The trainer's `chain_ledger.source` (E2, `w5-chain-ledger-source/v1`): re-read at every update."""
        source = {"schema": LEDGER_SOURCE_SCHEMA, "closed_usec": self.base, "instance": self.instance,
                  "cpu_stat": str(self.chain_cg.path / "cpu.stat")}
        if stop_at_usec is not None:
            source["stop_at_usec"] = int(stop_at_usec)
        write_json_atomic(self.ledger_source, source, durable=True)

    def last_stop_at(self) -> int | None:
        stops = [r["payload"]["stop_at_usec"] for r in self.ledger.records()  # the latest set or clear
                 if r["event"] == "resume" and isinstance(r.get("payload"), dict) and "stop_at_usec" in r["payload"]]
        return stops[-1] if stops else None

    # --- pause, filler, resume
    def spawn_filler(self, instance_dir: Path) -> None:
        filler = self.plan["filler"]
        seed_root = self.filler_seed_root()
        argv = filler_argv(filler, seed_root, self.slot["slot"], self.paused_seq)
        env = dict(BASE_ENV)
        env.update(self.plan.get("env", {}))
        env.update({"W5_FILLER_SEED_ROOT": seed_root, "W5_BOX_SLOT": str(self.slot["slot"])})
        self.filler_pid = spawn_into(self.filler_cg, argv, env,
                                     instance_dir / f"filler-{self.paused_seq}-{self.filler_spawns}.log")
        self.filler_spawns += 1

    def pause(self, request: dict, instance_dir: Path) -> None:
        # The paused mark and the freeze happen under the node lock, so enforcement never thaws a pausing chain.
        with self.node.locked():
            write_json_atomic(self.paused_path, {"instance": self.instance, "seq": int(request["seq"])},
                              durable=True)
            self.chain_cg.freeze(True)
        self.paused_usec = self.chain_cg.usage_usec()
        self.paused_seq = int(request["seq"])
        os.replace(self.control / "pause.request",
                   self.control / f"pause.{self.instance}.{self.paused_seq}.accepted")
        (self.control / "pause.intent").unlink(missing_ok=True)
        self.ledger.append({"event": "pause", "instance": self.instance, "seq": self.paused_seq,
                            "usage_usec": self.paused_usec, "request": request})
        make_leaf(self.box, "filler", int(self.plan["filler"]["memory_max_bytes"]))
        self.spawn_filler(instance_dir)

    def stop_filler(self) -> int | None:
        """Freeze, kill and remove the filler; its CPU (never the chain's)."""
        if not self.filler_cg.exists():
            return None
        self.filler_cg.freeze(True)
        self.filler_cg.kill()
        require(self.filler_cg.wait_empty(SETTLE_SECONDS), "Filler did not die")
        if self.filler_pid is not None:
            os.waitpid(self.filler_pid, 0)
            self.filler_pid = None
        usage = self.filler_cg.usage_usec()
        self.filler_cg.rmdir()
        return usage

    def resume(self, payload: dict) -> None:
        require(self.paused_seq is not None, "Chain is not paused")
        stop_at = payload.get("stop_at_usec")
        require(stop_at is None or (type(stop_at) is int and 0 < stop_at < 1 << 64),
                "stop_at_usec must be a positive u64 (or null to clear it)")
        filler_usec = self.stop_filler()
        if "stop_at_usec" in payload:  # null clears an earlier threshold
            self.write_ledger_source(stop_at)
        now = self.chain_cg.usage_usec()
        self.ledger.append({"event": "resume", "instance": self.instance, "seq": self.paused_seq,
                            "usage_usec": now, "unchanged_since_pause": now == self.paused_usec,
                            "filler_usage_usec": filler_usec, "payload": payload})
        write_json_atomic(self.control / f"resume.{self.instance}.{self.paused_seq}.json",
                          {"instance": self.instance, "seq": self.paused_seq, "payload": payload,
                           "ledger_usec": self.base + now}, durable=True)
        with self.node.locked():  # the resumed chain itself now occupies its box
            records, torn = self.node.read()
            state = NodeLog.state(records, torn)
            outage = state["open_outages"].get(self.slot["slot"])
            if outage is not None:
                self.node.append({"event": "restored", "outage_id": outage["id"], "box_slot": self.slot["slot"],
                                  "chain": self.chain, "how": "chain-resumed"})
        self.paused_path.unlink(missing_ok=True)
        self.paused_seq = self.paused_usec = None
        enforce_node(self.plan, self.node, "resume")  # thaws only if the node is clear

    def verify_filler(self) -> dict:
        """A new filler is alive, its process is in the box's filler cgroup and cpuset, and it burns CPU."""
        result, self.filler_pid = verify_filler(self.filler_cg, self.filler_pid, self.slot["cpus"])
        return result

    def clear_filler(self) -> bool:
        """Kill whatever is in the filler cgroup and confirm it is empty (no mixed old and new processes)."""
        self.filler_cg.kill()
        empty = self.filler_cg.wait_empty(SETTLE_SECONDS)
        if self.filler_pid is not None:
            try:
                os.waitpid(self.filler_pid, 0)
            except ChildProcessError:
                pass
            self.filler_pid = None
        return empty

    def restart_filler(self, instance_dir: Path) -> dict | None:
        """One verified replacement; None if the cgroup could not be emptied."""
        if not self.clear_filler():
            return None
        self.spawn_filler(instance_dir)
        verification = self.verify_filler()
        if not verification["ok"]:
            self.clear_filler()
        return verification

    def watch_filler(self, instance_dir: Path) -> None:
        """Occupancy is lost the moment the filler exits: the outage opens (the node freezes at once), restarts
        are verified, and the outage closes only on a verified replacement; else the box faults."""
        done, status = os.waitpid(self.filler_pid, os.WNOHANG)
        if done != self.filler_pid:
            return
        self.filler_pid = None
        code = os.waitstatus_to_exitcode(status)
        outage_id = f"{self.chain}/{self.instance}/{self.paused_seq}/{time.time_ns()}"
        with self.node.locked():
            self.node.append({"event": "outage-start", "id": outage_id, "box_slot": self.slot["slot"],
                              "chain": self.chain, "instance": self.instance, "seq": self.paused_seq, "exit": code,
                              "affected": node_chains(self.state_root)})
        enforce_node(self.plan, self.node, "outage")
        attempts = []
        for _ in range(FILLER_RESTARTS):
            verification = self.restart_filler(instance_dir)
            attempts.append(verification)
            if verification is not None and verification["ok"]:
                with self.node.locked():
                    self.node.append({"event": "restored", "outage_id": outage_id, "box_slot": self.slot["slot"],
                                      "chain": self.chain, "how": "restart", "attempts": attempts})
                enforce_node(self.plan, self.node, "restored")
                return
        with self.node.locked():
            self.node.append({"event": "fault", "outage_id": outage_id, "box_slot": self.slot["slot"],
                              "chain": self.chain, "instance": self.instance, "seq": self.paused_seq,
                              "attempts": attempts, "affected": node_chains(self.state_root),
                              "meaning": "box occupancy is not the frozen filler's; the node's chains stay frozen"})
        enforce_node(self.plan, self.node, "fault")

    def refill(self, instance_dir: Path) -> None:
        """Operator refill of a faulted box: an emptied cgroup, a new filler, verification, then the clear."""
        with self.node.locked():
            records, torn = self.node.read()
        state = NodeLog.state(records, torn)
        fault = state["open_faults"].get(self.slot["slot"])
        if fault is None or self.paused_seq is None:
            self.ledger.append({"event": "refill-refused", "instance": self.instance,
                                "reason": "no open fault in this box, or the chain is not paused"})
            return
        if not self.filler_cg.exists():
            make_leaf(self.box, "filler", int(self.plan["filler"]["memory_max_bytes"]))
        verification = self.restart_filler(instance_dir)
        with self.node.locked():
            if verification is not None and verification["ok"]:
                self.node.append({"event": "restored", "outage_id": fault["outage_id"], "box_slot": self.slot["slot"],
                                  "chain": self.chain, "how": "refill", "verification": verification})
            else:
                self.node.append({"event": "refill-failed", "outage_id": fault["outage_id"],
                                  "box_slot": self.slot["slot"], "chain": self.chain, "verification": verification})
        enforce_node(self.plan, self.node, "refill")

    def follow_node(self) -> None:
        """Copy new node events into this chain's ledger exactly once (the ledger's own last copied index is the
        watermark, so a crash cannot duplicate one), then enforce the node state."""
        with self.node.locked():
            records, _ = self.node.read()
        if self.node_copied is None:  # the chain ledger is the watermark: no separate file to fall out of step
            copied = [r["index"] for r in self.ledger.records() if r["event"] == "node-event"]
            self.node_copied = max(copied) + 1 if copied else 0
        if self.node_copied > len(records):
            raise RuntimeError("node log is shorter than this chain's copied events")
        for index in range(self.node_copied, len(records)):
            self.ledger.append({"event": "node-event", "instance": self.instance, "index": index,
                                "node": records[index]})
            self.node_copied = index + 1
        enforce_node(self.plan, self.node, "follow")

    # --- the run
    def run(self) -> int:
        self.state.mkdir(parents=True, exist_ok=True)
        locks = [flock(self.state / "supervisor.lock", f"Chain {self.chain}"),
                 flock(self.state_root / f"box{self.slot['slot']}.lock", f"Box {self.slot['slot']}")]
        with self.node.locked():  # concurrent supervisors create the shared root one at a time
            prepare_root(self.root, self.layout, self.plan["isolation"])
            prepare_box(self.root, self.slot)
        self.recover()
        if self.spec.get("handoff") is True:  # a W5 controller chain: an OOM kill is a chain failure, never resumed
            require(not any(r["event"] == "close" and r.get("chain_failure") for r in self.ledger.records()),
                    f"Chain {self.chain} was OOM-killed: a chain failure")
        summary = self.ledger.summary()
        if summary["unresolved_lower_bounds"]:
            require(bool(self.accept_lower_bound), f"Chain {self.chain} has lower-bound instances "
                    f"{summary['unresolved_lower_bounds']}; continuing needs --accept-lower-bound REASON")
            for instance in summary["unresolved_lower_bounds"]:
                self.ledger.append({"event": "accept-lower-bound", "instance": instance,
                                    "reason": self.accept_lower_bound})
        if self.control.exists():  # earlier instances' handshakes never reach this one
            for stale in self.control.iterdir():
                stale.unlink()
        self.control.mkdir(parents=True, exist_ok=True)
        self.instance, self.base = summary["next_instance"], summary["closed_total_usec"]
        instance_dir = self.state / f"instance-{self.instance:04d}"
        argv = chain_argv(self.spec, self.chain, self.state)
        make_leaf(self.box, "chain", int(self.spec["memory_max_bytes"]))
        manifest_path = instance_dir / "manifest.json"
        write_json_atomic(manifest_path, self.manifest(argv), durable=True)
        write_json_atomic(self.owner_path, {"chain": self.chain, "instance": self.instance}, durable=True)
        self.write_ledger_source(self.last_stop_at())
        self.ledger.append({"event": "open", "instance": self.instance, "base_usec": self.base,
                            "cgroup": str(self.chain_cg.path), "cgroup_inode": self.chain_cg.inode(),
                            "manifest_sha256": sha256_file(manifest_path)})
        with self.node.locked():  # the start is decided and spawned under the lock
            records, torn = self.node.read()
            state = NodeLog.state(records, torn)
            own = state["open_outages"].get(self.slot["slot"])
            foreign = ({k for k in state["open_outages"] if k != self.slot["slot"]}
                       | {k for k in state["open_faults"] if k != self.slot["slot"]})
            held = bool(foreign) or state["operator_freeze"] is not None or state["torn_tail"] or \
                state["torn_hold"] is not None
            # No measured execution while any other box is unoccupied: the chain starts frozen and the node's
            # enforcement thaws it once every box is restored.
            self.pid = spawn_into(self.chain_cg, argv, self.env(manifest_path), instance_dir / "chain.log",
                                  self.spec.get("cwd"), frozen=held)
            if own is not None:  # spawn_into returned only after the chain joined its cgroup: the box is occupied
                self.node.append({"event": "restored", "outage_id": own["id"], "box_slot": self.slot["slot"],
                                  "chain": self.chain, "how": "chain-restart",
                                  "verification": {"joined_chain_cgroup": True, "started_frozen": held}})
        self.ledger.append({"event": "start", "instance": self.instance, "started_frozen": held})
        enforce_node(self.plan, self.node, "start")
        started = time.monotonic()
        stop = {"flag": False}
        signal.signal(signal.SIGTERM, lambda *_: stop.__setitem__("flag", True))
        status, ru, last_live, reason = None, None, 0.0, None
        while True:
            done, wstatus, ru_now = os.wait4(self.pid, os.WNOHANG)
            if done == self.pid:
                status, ru = wstatus, ru_now
                break
            request_path = self.control / "pause.request"
            if self.paused_seq is None and (self.control / "pause.now").exists():
                accepted = sorted(int(x.name.split(".")[2])
                                  for x in self.control.glob(f"pause.{self.instance}.*.accepted"))
                now_request = dict(json.loads((self.control / "pause.now").read_text()), instance=self.instance,
                                   seq=accepted[-1] + 1 if accepted else 0, at="controller")
                os.replace(self.control / "pause.now", request_path)
                self.pause(now_request, instance_dir)
            elif self.paused_seq is None and request_path.exists():
                request = json.loads(request_path.read_text())
                if (request.get("instance") == self.instance and type(request.get("seq")) is int and not
                        (self.control / f"pause.{self.instance}.{request['seq']}.accepted").exists()):
                    self.pause(request, instance_dir)
                else:
                    os.replace(request_path, self.control / f"pause.rejected.{time.time_ns()}")
                    self.ledger.append({"event": "pause-rejected", "instance": self.instance, "request": request})
            elif self.paused_seq is not None and (self.control / "resume.request").exists():
                payload = json.loads((self.control / "resume.request").read_text())
                (self.control / "resume.request").unlink()
                self.resume(payload)
            if (self.control / "kill.request").exists() or stop["flag"]:
                reason = reason or ("sigterm" if stop["flag"] else "kill-request")
                (self.control / "kill.request").unlink(missing_ok=True)
                self.chain_cg.kill()
            if self.max_wall is not None and time.monotonic() - started > self.max_wall and reason is None:
                reason = "qualification-wall-cap"
                self.chain_cg.kill()
            if self.filler_pid is not None:
                self.watch_filler(instance_dir)
            if (self.control / "refill.request").exists():
                (self.control / "refill.request").unlink()
                self.refill(instance_dir)
            self.follow_node()
            if time.monotonic() - last_live >= LIVE_EVERY_SECONDS:
                last_live = time.monotonic()
                write_json_atomic(instance_dir / "live.json", {"usage_usec": self.chain_cg.usage_usec(),
                                                               "base_usec": self.base, "unix": time.time()},
                                  durable=True)
            time.sleep(POLL_SECONDS)
        events_now = self.chain_cg.keyed("memory.events")
        if self.spec.get("handoff") is True or \
                int(events_now.get("oom_kill", 0)) + int(events_now.get("oom_group_kill", 0)):
            # The box empties: it is an outage, with the node's other measured chains frozen, from now (before a
            # paused filler goes or the box is released) until the chain's restart or a verified filler holds it.
            with self.node.locked():
                records, torn = self.node.read()
                if self.slot["slot"] not in NodeLog.state(records, torn)["open_outages"]:
                    self.node.append({"event": "outage-start", "box_slot": self.slot["slot"], "handoff": True,
                                      "id": f"{self.chain}/{self.instance}/handoff/{time.time_ns()}",
                                      "chain": self.chain, "instance": self.instance,
                                      "reason": "the chain's instance closed: its box waits to be occupied",
                                      "affected": node_chains(self.state_root)})
            enforce_node(self.plan, self.node, "handoff")
        if self.paused_seq is not None or self.chain_cg.frozen():  # the chain died while paused or frozen (a kill)
            self.stop_filler()
            self.paused_path.unlink(missing_ok=True)
            self.chain_cg.freeze(False)
        if not self.chain_cg.wait_empty(SETTLE_SECONDS):  # orphans outlived the chain's root process
            reason = reason or "orphans-killed"
            self.chain_cg.kill()
            if not self.chain_cg.wait_empty(SETTLE_SECONDS):
                self.ledger.append({"event": "cleanup-timeout", "instance": self.instance,
                                    "usage_usec": self.chain_cg.usage_usec()})
                return EXIT_CLEANUP  # the instance stays open: recovery closes it once the cgroup is empty
        final = self.chain_cg.usage_usec()
        events = self.chain_cg.keyed("memory.events")
        oom = int(events.get("oom_kill", 0)) + int(events.get("oom_group_kill", 0))
        code = os.waitstatus_to_exitcode(status)
        close = {"final_usec": final, "ledger_usec": self.base + final, "exit": code, "oom_kill": oom,
                 "reason": reason, "root_rusage_usec": rusage_usec(ru), "memory_events": events}
        self.ledger.append({"event": "close", "instance": self.instance, "final_usec": final,
                            "closed_total_usec": self.base + final, "exit": code, "oom_kill": oom,
                            "root_rusage_usec": rusage_usec(ru), "reason": reason,
                            "chain_failure": bool(oom) or None})
        write_json_atomic(instance_dir / "close.json", close, durable=True)
        self.chain_cg.rmdir()
        self.owner_path.unlink(missing_ok=True)
        for lock in locks:
            lock.close()
        if oom:
            return EXIT_OOM
        return code if code >= 0 else 128 - code


# ---------------------------------------------------------------- recovery without a restart

def recover_chain(plan: dict, plan_sha: str, chain: str, layout: dict, detected: str) -> dict:
    """`w5box.py recover`: closes the open instance of a chain whose supervisor has died (module docstring)."""
    sup = Supervisor(plan, plan_sha, chain, None, None, layout, detected, None)
    if not sup.ledger.path.exists() or sup.ledger.summary()["open"] is None:
        return {"chain": chain, "recovered": False, "why": "no open instance"}
    locks = [flock(sup.state / "supervisor.lock", f"Chain {chain}"),
             flock(sup.state_root / f"box{sup.slot['slot']}.lock", f"Box {sup.slot['slot']}")]
    try:
        killed = False
        if sup.chain_cg.exists() and sup.chain_cg.populated():  # unsupervised: nothing reads its requests
            # Killed as it is: a paused (frozen) chain never runs again beside its box's filler.
            sup.chain_cg.kill()
            require(sup.chain_cg.wait_empty(SETTLE_SECONDS), f"{sup.chain_cg.path} did not empty")
            killed = True
        sup.recover(killed_running=killed)  # a handoff chain's box is held there, before the instance closes
        return {"chain": chain, "recovered": True, "killed": killed}
    finally:
        for lock in locks:
            lock.close()


# ---------------------------------------------------------------- an ended chain's box

def fill_box(plan: dict, index: int, layout: dict, empty_since: float | None, stop_path: Path) -> int:
    """`w5box.py fill`: the frozen filler in an ended chain's box, with the paused-box filler's outage, verification,
    replacement and fault rules (module docstring, "Ended boxes")."""
    state_root = Path(plan["state_root"])
    slot = layout["boxes"][index]
    node = NodeLog(state_root)
    stop_key = str(stop_path)  # each outage names its stage's stop file: only that stage's fills close it
    root = Cgroup(plan["cgroup_root"])
    with node.locked():
        prepare_root(root, layout, plan["isolation"])
        box = prepare_box(root, slot)
    chain_cg, filler_cg = Cgroup(box.path / "chain"), Cgroup(box.path / "filler")
    require(not chain_cg.exists() or not chain_cg.populated(), f"Box {index} holds a running chain")
    filler = plan["filler"]
    seed_root = filler["seed_root_template"].format(j=index)
    env = dict(BASE_ENV)
    env.update(plan.get("env", {}))
    env.update({"W5_FILLER_SEED_ROOT": seed_root, "W5_BOX_SLOT": str(index)})
    mark = state_root / f"box{index}.fill.json"
    stop = {"flag": False}
    signal.signal(signal.SIGTERM, lambda *_: stop.__setitem__("flag", True))

    def open_outage(reason: str, **extra) -> None:
        with node.locked():
            records, torn = node.read()
            if index not in NodeLog.state(records, torn)["open_outages"]:
                node.append(dict({"event": "outage-start", "id": f"fill{index}/{time.time_ns()}", "box_slot": index,
                                  "chain": None, "reason": reason, "stop": stop_key,
                                  "affected": node_chains(state_root)}, **extra))
        enforce_node(plan, node, "fill-outage")

    def close_outage(how: str, any_source: bool = False, **extra) -> None:
        """Closes this box's open outage if this stage's fill opened it, or any open outage (and fault) of the box
        once a verified filler holds it (an outage inherited from the paused-box filler included)."""
        with node.locked():
            records, torn = node.read()
            outage = NodeLog.state(records, torn)["open_outages"].get(index)
            if outage is not None and (any_source or outage.get("stop") == stop_key):
                node.append(dict({"event": "restored", "outage_id": outage["id"], "box_slot": index, "chain": None,
                                  "how": how}, **extra))
        enforce_node(plan, node, how)

    if stop_path.exists():  # the stage has ended: nothing waits on this box (an outage left by this stage closes)
        close_outage("fill-stopped")
        return 0

    def clear(pid: int | None) -> None:
        if filler_cg.exists():
            filler_cg.kill()
            require(filler_cg.wait_empty(SETTLE_SECONDS), f"Box {index}: the filler did not die")
            if pid is not None:
                try:
                    os.waitpid(pid, 0)
                except ChildProcessError:
                    pass
            filler_cg.rmdir()

    # The box is empty: the outage opens before the wait for the box lock (the ended chain's supervisor may still
    # hold it for a moment after its ledger closes), and stays open if the lock never comes.
    open_outage("ended box: waiting for the frozen filler", empty_since=empty_since)
    deadline = time.monotonic() + SETTLE_SECONDS
    while True:
        if stop_path.exists():
            close_outage("fill-stopped")
            return 0
        try:
            lock = flock(state_root / f"box{index}.lock", f"Box {index}")
            break
        except Refusal:
            if chain_cg.exists() and chain_cg.populated():  # another chain took the box: it is not empty
                close_outage("box-occupied-by-a-chain")
                raise
            if time.monotonic() > deadline:
                raise  # the box stays an outage; the next start retries
            time.sleep(0.5)
    if chain_cg.exists() and chain_cg.populated():
        close_outage("box-occupied-by-a-chain")
        lock.close()
        raise Refusal(f"Box {index} holds a running chain")
    write_json_atomic(mark, {"pid": os.getpid(), "unix": time.time(), "empty_since": empty_since, "stop": stop_key},
                      durable=True)
    pid, seq, failures, faulted, ended = None, 0, 0, False, False
    try:
        while not stop["flag"]:
            if stop_path.exists():
                ended = True
                break
            if pid is not None:
                done, status = os.waitpid(pid, os.WNOHANG)
                if done == pid:
                    pid = None
                    open_outage("the box filler exited", exit=os.waitstatus_to_exitcode(status))
            if pid is None and not faulted:
                clear(None)
                make_leaf(box, "filler", int(filler["memory_max_bytes"]))
                pid = spawn_into(filler_cg, filler_argv(filler, seed_root, index, seq), env,
                                 state_root / f"box{index}.fill" / f"filler-{seq}.log")
                seq += 1
                verification, pid = verify_filler(filler_cg, pid, slot["cpus"])
                if verification["ok"]:
                    failures = 0
                    close_outage("fill-verified", any_source=True, verification=verification)
                else:
                    failures += 1
                    clear(pid)
                    pid = None
                    if failures >= FILLER_RESTARTS:
                        faulted = True
                        with node.locked():
                            records, torn = node.read()
                            outage = NodeLog.state(records, torn)["open_outages"].get(index)
                            node.append({"event": "fault", "outage_id": outage and outage["id"], "box_slot": index,
                                         "chain": None, "attempts": failures, "affected": node_chains(state_root),
                                         "meaning": "the ended box holds no verified filler; the node's chains stay "
                                                    "frozen"})
            enforce_node(plan, node, "fill")  # escalation of an outage left open
            time.sleep(FILL_POLL_SECONDS)
    finally:
        if ended:  # every measured chain has ended: nothing waits on this box any more
            clear(pid)
            close_outage("fill-stopped")
        else:  # a signal or an error: the box is about to be empty, so the node holds first
            open_outage("the box fill stopped")
            clear(pid)
        mark.unlink(missing_ok=True)
        lock.close()
    return 0


# ---------------------------------------------------------------- commands

def guarded_supervisor(args) -> Supervisor:
    require(sys.platform.startswith("linux"), "The W5 launcher runs on Linux only")
    plan, plan_sha = load_plan(args.plan)
    spec = chain_spec(plan, args.chain)
    frozen = plan["layout"]
    layout = compute_layout(read_topology(), frozen["cores_per_box"], frozen["offbox_cores_per_domain"],
                            len(frozen["boxes"]))
    detected = host_class()
    check_pins(plan, args.chain, spec, layout, detected)
    throughput = None
    ref = plan.get("throughput")
    if ref is not None:
        require(sha256_file(ref["path"]) == ref["sha256"], "Throughput record hash changed")
        throughput = dict(require_throughput(json.loads(Path(ref["path"]).read_text()),
                                             throughput_binding(plan, spec)), sha256=ref["sha256"])
    if args.qualification:
        cap = QUALIFICATION_MAX_WALL if throughput else UNQUALIFIED_MAX_WALL
        require(args.max_wall_seconds is not None and 0 < args.max_wall_seconds <= cap,
                f"A qualification run needs --max-wall-seconds within {cap} s"
                + ("" if throughput else " without a box throughput record"))
    else:
        require(args.max_wall_seconds is None, "--max-wall-seconds is for qualification runs")
        require(throughput is not None, "Plan names no box throughput record (COMPUTE-POLICY item 5)")
    return Supervisor(plan, plan_sha, args.chain, args.qualification, args.max_wall_seconds, layout, detected,
                      throughput, args.accept_lower_bound)


def request(args, name: str) -> int:
    plan, _ = load_plan(args.plan)
    chain_spec(plan, args.chain)
    control = Path(plan["state_root"]) / args.chain / "control"
    require(control.exists(), f"Chain {args.chain} has no supervisor state")
    payload = json.loads(args.payload) if getattr(args, "payload", None) else {}
    write_json_atomic(control / name, dict(payload, by="operator", unix=time.time()))
    print(f"wrote {control / name}")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("layout")
    p.add_argument("--cores-per-box", type=int, default=2)
    p.add_argument("--offbox-cores-per-domain", type=int, default=2)
    p.add_argument("--boxes", type=int, default=None)
    p.add_argument("--out", type=Path, default=None)
    sub.add_parser("host-class")
    p = sub.add_parser("run")
    p.add_argument("--plan", required=True)
    p.add_argument("--chain", required=True)
    p.add_argument("--qualification", default=None)
    p.add_argument("--max-wall-seconds", type=float, default=None)
    p.add_argument("--accept-lower-bound", default=None, metavar="REASON")
    for name in ("node-freeze", "node-thaw"):
        p = sub.add_parser(name)
        p.add_argument("--plan", required=True)
        p.add_argument("--reason", required=True)
    p = sub.add_parser("occupancy")
    p.add_argument("--plan", required=True)
    p = sub.add_parser("fill")
    p.add_argument("--plan", required=True)
    p.add_argument("--slot", type=int, required=True)
    p.add_argument("--stop", type=Path, required=True)
    p.add_argument("--empty-since", type=float, default=None)
    for name in ("pause", "resume", "kill", "refill", "ledger", "verify-ledger", "recover"):
        p = sub.add_parser(name)
        p.add_argument("--plan", required=True)
        p.add_argument("--chain", required=True)
        if name in ("pause", "resume"):
            p.add_argument("--payload", default=None)
        if name == "pause":
            p.add_argument("--now", action="store_true", help="freeze immediately (the controller's 1.10 R-hat pause)")
    args = parser.parse_args(argv)
    try:
        if args.command == "layout":
            layout = compute_layout(read_topology(), args.cores_per_box, args.offbox_cores_per_domain, args.boxes)
            text = json.dumps(dict(layout, host_class=host_class()), indent=2) + "\n"
            if args.out:
                args.out.write_text(text, encoding="utf-8", newline="\n")
            print(text, end="")
            return 0
        if args.command == "host-class":
            print(host_class())
            return 0
        if args.command == "run":
            return guarded_supervisor(args).run()
        if args.command == "recover":
            require(sys.platform.startswith("linux"), "The W5 launcher runs on Linux only")
            plan, plan_sha = load_plan(args.plan)
            chain_spec(plan, args.chain)
            frozen = plan["layout"]
            layout = compute_layout(read_topology(), frozen["cores_per_box"], frozen["offbox_cores_per_domain"],
                                    len(frozen["boxes"]))
            require({k: frozen.get(k) for k in LAYOUT_KEYS} == {k: layout[k] for k in LAYOUT_KEYS},
                    "The plan's box layout differs from the layout detected on this host")
            print(json.dumps(recover_chain(plan, plan_sha, args.chain, layout, host_class())))
            return 0
        if args.command == "fill":
            require(sys.platform.startswith("linux"), "The W5 launcher runs on Linux only")
            plan, _ = load_plan(args.plan)
            frozen = plan["layout"]
            layout = compute_layout(read_topology(), frozen["cores_per_box"], frozen["offbox_cores_per_domain"],
                                    len(frozen["boxes"]))
            require(host_class() == plan["host_class"], "Host class differs from the plan's")
            require({k: frozen.get(k) for k in LAYOUT_KEYS} == {k: layout[k] for k in LAYOUT_KEYS},
                    "The plan's box layout differs from the layout detected on this host")
            require(0 <= args.slot < len(layout["boxes"]), "Box slot is not in the layout")
            filler = plan["filler"]
            check_launch(filler, filler_argv(filler, filler["seed_root_template"].format(j=args.slot), args.slot, 0),
                         "filler")
            return fill_box(plan, args.slot, layout, args.empty_since, args.stop)
        if args.command == "pause":  # --now: freeze at once; else an intent the trainer takes at a boundary
            return request(args, "pause.now" if args.now else "pause.intent")
        if args.command in ("resume", "kill", "refill"):
            return request(args, f"{args.command}.request")
        if args.command in ("node-freeze", "node-thaw", "occupancy"):
            plan, _ = load_plan(args.plan)
            node = NodeLog(Path(plan["state_root"]))
            if args.command != "occupancy":
                with node.locked():
                    node.append({"event": args.command, "by": "operator", "reason": args.reason,
                                 "affected": node_chains(Path(plan["state_root"]))})
            state = enforce_node(plan, node, args.command)
            with node.locked():
                records, _ = node.read()
            outages = [r for r in records if r["event"] in ("outage-start", "restored", "fault", "refill-failed",
                                                            "escalate", "filler-replaced-by-chain")]
            print(json.dumps(dict(state, outages=outages), indent=2, default=str))
            return 0
        plan, _ = load_plan(args.plan)
        spec = chain_spec(plan, args.chain)
        ledger = Ledger(Path(plan["state_root"]) / args.chain / "ledger.jsonl")
        if args.command == "verify-ledger":
            print(json.dumps(ledger.summary(), indent=2))
            return 0
        cg = Cgroup(Path(plan["cgroup_root"]) / f"box{spec['box_slot']}" / "chain")
        print(json.dumps(ledger_value(ledger, cg), indent=2))
        return 0
    except Refusal as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return EXIT_REFUSED


if __name__ == "__main__":
    raise SystemExit(main())
