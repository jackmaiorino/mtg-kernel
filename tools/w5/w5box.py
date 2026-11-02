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
- `W5_CONTROL_DIR`: the pause handshake. At an update boundary the trainer writes `pause.request`
  (JSON `{"seq": n, ...}`, written to a temporary name and renamed) and then only polls for `resume.<n>.json`.
  The launcher freezes the chain cgroup, records the frozen value and starts the filler. `resume` freezes and
  kills the filler, writes `resume.<n>.json` (with the operator's payload, e.g. R_i) and thaws the chain.
- `W5_BOX_SLOT`, `W5_BOX_CPUS`, `W5_HOST_CLASS`, `W5_CHAIN`, `W5_MANIFEST`: identity for receipts.

**Guard (COMPUTE-POLICY item 5).** `run` refuses before any spawn unless the plan's pins hold (host class, box
layout positions, binary and config hashes) and a box throughput record binds this plan (`w5-box-throughput/v1`:
serial and full-node concurrent phases with identical outputs). A qualification run (Stage 0) runs without a
record only with `--qualification LABEL --max-wall-seconds S`, and its manifest says so.

    w5box.py layout [--cores-per-box 2] [--offbox-cores-per-domain 2] [--out LAYOUT.json]
    w5box.py host-class
    w5box.py run --plan PLAN.json --chain CHAIN [--qualification LABEL --max-wall-seconds S]
    w5box.py pause|resume|kill --plan PLAN.json --chain CHAIN [--payload JSON]
    w5box.py ledger --plan PLAN.json --chain CHAIN       current ledger value (closed + live)
    w5box.py verify-ledger --plan PLAN.json --chain CHAIN

Linux with cgroup v2 only; `run` needs write access to the cgroup root (root, or a delegated subtree).
"""
from __future__ import annotations

import argparse
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
LIVE_EVERY_SECONDS = 30.0
SETTLE_SECONDS = 60.0
FREEZE_SECONDS = 30.0
QUALIFICATION_MAX_WALL = 8 * 86400
EXIT_OOM = 3
EXIT_REFUSED = 2


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


def write_json_atomic(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_name(path.name + ".tmp")
    tmp.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    os.replace(tmp, path)


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


def spawn_into(cgroup: Cgroup, argv: list[str], env: dict, log_path: Path, cwd: str | None = None) -> int:
    """Fork, move the child into the cgroup before exec, so every descendant is metered there."""
    log_path.parent.mkdir(parents=True, exist_ok=True)
    log_fd = os.open(log_path, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
    procs = str(cgroup.path / "cgroup.procs")
    pid = os.fork()
    if pid == 0:  # child: only async-signal-safe-ish work, then exec
        try:
            with open(procs, "w") as handle:
                handle.write(str(os.getpid()))
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
    return pid


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
        closed, open_instance, instances = 0, None, 0
        for record in self.records():
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
        return {"closed_total_usec": closed, "open": open_instance, "next_instance": instances}


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
    return plan, sha256_bytes(raw)


def chain_spec(plan: dict, chain: str) -> dict:
    spec = plan["chains"].get(chain)
    require(spec is not None, f"Plan has no chain {chain}")
    for key in ("box_slot", "arm", "argv", "binary", "config", "memory_max_bytes", "seed_root"):
        require(key in spec, f"Chain {chain} lacks {key}")
    return spec


def check_pins(plan: dict, spec: dict, layout: dict, detected_class: str) -> None:
    require(detected_class == plan["host_class"],
            f"Host class {detected_class} differs from the plan's {plan['host_class']}")
    require(layout["positions_sha256"] == plan["layout"]["positions_sha256"],
            "Box positions differ from the plan's frozen layout")
    for key in ("binary", "config"):
        require(sha256_file(spec[key]["path"]) == spec[key]["sha256"], f"{key} hash changed: {spec[key]['path']}")
    filler = plan["filler"]
    for key in ("argv", "binary", "config", "memory_max_bytes", "seed_root_template"):
        require(key in filler, f"Filler lacks {key}")
    for key in ("binary", "config"):
        require(sha256_file(filler[key]["path"]) == filler[key]["sha256"], f"filler {key} hash changed")
    require(int(spec["memory_max_bytes"]) > 0 and int(filler["memory_max_bytes"]) > 0, "Memory limits required")


def throughput_binding(plan: dict, spec: dict) -> dict:
    return {"host_class": plan["host_class"], "positions_sha256": plan["layout"]["positions_sha256"],
            "cores_per_box": plan["layout"]["cores_per_box"], "boxes": len(plan["layout"]["boxes"]),
            "arm": spec["arm"], "binary_sha256": spec["binary"]["sha256"], "config_sha256": spec["config"]["sha256"]}


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

class Supervisor:
    def __init__(self, plan: dict, plan_sha: str, chain: str, qualification: str | None,
                 max_wall: float | None, layout: dict, detected_class: str, throughput: dict | None):
        self.plan, self.plan_sha, self.chain = plan, plan_sha, chain
        self.spec = chain_spec(plan, chain)
        self.slot = plan["layout"]["boxes"][self.spec["box_slot"]]
        self.qualification, self.max_wall = qualification, max_wall
        self.layout, self.detected_class, self.throughput = layout, detected_class, throughput
        self.state = Path(plan["state_root"]) / chain
        self.control = self.state / "control"
        self.ledger = Ledger(self.state / "ledger.jsonl")
        self.root = Cgroup(plan["cgroup_root"])
        self.box = Cgroup(self.root.path / f"box{self.slot['slot']}")
        self.chain_cg = Cgroup(self.box.path / "chain")
        self.filler_cg = Cgroup(self.box.path / "filler")
        self.pid = None
        self.filler_pid = None
        self.paused_seq = None
        self.paused_usec = None
        self.filler_usec_total = 0

    # --- recovery of an instance whose supervisor died
    def recover(self) -> None:
        if self.filler_cg.exists():  # a filler left by a supervisor that died while its chain was paused
            self.filler_cg.kill()
            self.filler_cg.wait_empty(SETTLE_SECONDS)
            self.filler_cg.rmdir()
        summary = self.ledger.summary()
        record = summary["open"]
        if record is None:
            require(not self.chain_cg.exists() or not self.chain_cg.populated(),
                    f"{self.chain_cg.path} is populated but the ledger has no open instance")
            if self.chain_cg.exists():
                self.chain_cg.rmdir()
            return
        if self.chain_cg.exists() and self.chain_cg.inode() == record["cgroup_inode"]:
            require(not self.chain_cg.populated(), f"Chain {self.chain} instance is still running")
            final, how = self.chain_cg.usage_usec(), "cgroup"
        else:
            live = self.state / f"instance-{record['instance']:04d}" / "live.json"
            final = json.loads(live.read_text())["usage_usec"] if live.exists() else 0
            how = "last-live-reading (cgroup lost; a lower bound)"
        self.ledger.append({"event": "close", "instance": record["instance"], "final_usec": final,
                            "closed_total_usec": summary["closed_total_usec"] + final,
                            "exit": None, "oom_kill": None, "recovered": how})
        if self.chain_cg.exists():
            self.chain_cg.rmdir()

    def manifest(self, instance: int, base: int) -> dict:
        spec = self.spec
        return {"schema": MANIFEST_SCHEMA, "chain": self.chain, "arm": spec["arm"], "instance": instance,
                "base_usec": base, "host_class": self.detected_class, "hostname": platform.node(),
                "kernel": platform.release(), "box_slot": self.slot["slot"], "box_cpus": self.slot["cpus"],
                "box_cores": self.slot["cores"], "layout_positions_sha256": self.layout["positions_sha256"],
                "cgroup_root": str(self.root.path), "isolation": self.plan["isolation"],
                "memory_max_bytes": int(spec["memory_max_bytes"]), "argv": spec["argv"],
                "binary": spec["binary"], "config": spec["config"], "seed_root": spec["seed_root"],
                "filler": {"config_sha256": self.plan["filler"]["config"]["sha256"],
                           "binary_sha256": self.plan["filler"]["binary"]["sha256"],
                           "seed_root": self.filler_seed_root(),
                           "memory_max_bytes": int(self.plan["filler"]["memory_max_bytes"])},
                "plan_sha256": self.plan_sha, "launcher_sha256": sha256_file(__file__),
                "qualification": self.qualification, "max_wall_seconds": self.max_wall,
                "throughput": self.throughput, "started_unix": round(time.time(), 3)}

    def filler_seed_root(self) -> str:
        return self.plan["filler"]["seed_root_template"].format(j=self.slot["slot"])

    def env(self, instance: int, base: int, manifest_path: Path) -> dict:
        env = dict(os.environ)
        env.update(self.plan.get("env", {}))
        env.update({"W5_LEDGER_CGROUP": str(self.chain_cg.path), "W5_LEDGER_BASE_USEC": str(base),
                    "W5_LEDGER_INSTANCE": str(instance), "W5_CONTROL_DIR": str(self.control),
                    "W5_BOX_SLOT": str(self.slot["slot"]), "W5_BOX_CPUS": format_cpu_list(self.slot["cpus"]),
                    "W5_HOST_CLASS": self.detected_class, "W5_CHAIN": self.chain,
                    "W5_MANIFEST": str(manifest_path)})
        return env

    # --- pause, filler, resume
    def pause(self, request: dict, instance_dir: Path) -> None:
        self.chain_cg.freeze(True)
        self.paused_usec = self.chain_cg.usage_usec()
        self.paused_seq = int(request["seq"])
        os.replace(self.control / "pause.request", self.control / f"pause.{self.paused_seq}.accepted")
        self.ledger.append({"event": "pause", "instance": self.instance, "seq": self.paused_seq,
                            "usage_usec": self.paused_usec, "request": request})
        filler = self.plan["filler"]
        make_leaf(self.box, "filler", int(filler["memory_max_bytes"]))
        seed_root = self.filler_seed_root()
        argv = [a.format(seed_root=seed_root, slot=self.slot["slot"], seq=self.paused_seq) for a in filler["argv"]]
        env = dict(os.environ, W5_FILLER_SEED_ROOT=seed_root, W5_BOX_SLOT=str(self.slot["slot"]))
        self.filler_pid = spawn_into(self.filler_cg, argv, env, instance_dir / f"filler-{self.paused_seq}.log")

    def resume(self, payload: dict) -> None:
        require(self.paused_seq is not None, "Chain is not paused")
        filler_usec = None
        if self.filler_cg.exists():
            self.filler_cg.freeze(True)
            self.filler_cg.kill()
            require(self.filler_cg.wait_empty(SETTLE_SECONDS), "Filler did not die")
            self.reap_filler()
            filler_usec = self.filler_cg.usage_usec()
            self.filler_cg.rmdir()
        now = self.chain_cg.usage_usec()
        unchanged = now == self.paused_usec
        self.ledger.append({"event": "resume", "instance": self.instance, "seq": self.paused_seq,
                            "usage_usec": now, "unchanged_since_pause": unchanged,
                            "filler_usage_usec": filler_usec, "payload": payload})
        write_json_atomic(self.control / f"resume.{self.paused_seq}.json",
                          {"seq": self.paused_seq, "payload": payload, "ledger_usec": self.base + now})
        self.chain_cg.freeze(False)
        self.paused_seq = self.paused_usec = None

    def reap_filler(self) -> None:
        if self.filler_pid is not None:
            try:
                os.waitpid(self.filler_pid, 0)
            except ChildProcessError:
                pass
            self.filler_pid = None

    # --- the run
    def run(self) -> int:
        self.state.mkdir(parents=True, exist_ok=True)
        self.control.mkdir(parents=True, exist_ok=True)
        import fcntl
        lock = open(self.state / "supervisor.lock", "w")
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError:
            raise Refusal(f"Another supervisor holds chain {self.chain}")
        for stale in self.control.glob("*.request"):
            stale.unlink()
        prepare_root(self.root, self.layout, self.plan["isolation"])
        prepare_box(self.root, self.slot)
        self.recover()
        summary = self.ledger.summary()
        self.instance, self.base = summary["next_instance"], summary["closed_total_usec"]
        instance_dir = self.state / f"instance-{self.instance:04d}"
        make_leaf(self.box, "chain", int(self.spec["memory_max_bytes"]))
        manifest = self.manifest(self.instance, self.base)
        manifest_path = instance_dir / "manifest.json"
        write_json_atomic(manifest_path, manifest)
        self.ledger.append({"event": "open", "instance": self.instance, "base_usec": self.base,
                            "cgroup": str(self.chain_cg.path), "cgroup_inode": self.chain_cg.inode(),
                            "manifest_sha256": sha256_file(manifest_path)})
        started = time.monotonic()
        self.pid = spawn_into(self.chain_cg, self.spec["argv"], self.env(self.instance, self.base, manifest_path),
                              instance_dir / "chain.log", self.spec.get("cwd"))
        stop = {"flag": False}
        signal.signal(signal.SIGTERM, lambda *_: stop.__setitem__("flag", True))
        status, ru, last_live, reason = None, None, 0.0, None
        while True:
            done, wstatus, ru_now = os.wait4(self.pid, os.WNOHANG)
            if done == self.pid:
                status, ru = wstatus, ru_now
                break
            if self.paused_seq is None and (self.control / "pause.request").exists():
                self.pause(json.loads((self.control / "pause.request").read_text()), instance_dir)
            elif self.paused_seq is not None and (self.control / "resume.request").exists():
                payload = json.loads((self.control / "resume.request").read_text())
                (self.control / "resume.request").unlink()
                self.resume(payload)
            if (self.control / "kill.request").exists() or stop["flag"]:
                reason = reason or ("kill-request" if not stop["flag"] else "sigterm")
                (self.control / "kill.request").unlink(missing_ok=True)
                self.chain_cg.kill()
            if self.max_wall is not None and time.monotonic() - started > self.max_wall and reason is None:
                reason = "qualification-wall-cap"
                self.chain_cg.kill()
            if self.filler_pid is not None:
                fdone, _ = os.waitpid(self.filler_pid, os.WNOHANG)
                if fdone == self.filler_pid:
                    self.filler_pid = None
                    self.ledger.append({"event": "filler-exited", "instance": self.instance, "seq": self.paused_seq})
            if time.monotonic() - last_live >= LIVE_EVERY_SECONDS:
                last_live = time.monotonic()
                write_json_atomic(instance_dir / "live.json", {"usage_usec": self.chain_cg.usage_usec(),
                                                               "base_usec": self.base, "unix": time.time()})
            time.sleep(POLL_SECONDS)
        if self.paused_seq is not None and self.filler_cg.exists():  # the chain died while paused
            self.filler_cg.kill()
            self.filler_cg.wait_empty(SETTLE_SECONDS)
            self.reap_filler()
            self.filler_cg.rmdir()
            self.chain_cg.freeze(False)
        if not self.chain_cg.wait_empty(SETTLE_SECONDS):  # orphans outlived the chain's root process
            reason = reason or "orphans-killed"
            self.chain_cg.kill()
            self.chain_cg.wait_empty(SETTLE_SECONDS)
        final = self.chain_cg.usage_usec()
        events = self.chain_cg.keyed("memory.events")
        oom = int(events.get("oom_kill", 0)) + int(events.get("oom_group_kill", 0))
        code = os.waitstatus_to_exitcode(status)
        self.ledger.append({"event": "close", "instance": self.instance, "final_usec": final,
                            "closed_total_usec": self.base + final, "exit": code, "oom_kill": oom,
                            "root_rusage_usec": rusage_usec(ru), "reason": reason,
                            "chain_failure": bool(oom) or None})
        write_json_atomic(instance_dir / "close.json", {"final_usec": final, "ledger_usec": self.base + final,
                                                        "exit": code, "oom_kill": oom, "reason": reason,
                                                        "root_rusage_usec": rusage_usec(ru),
                                                        "memory_events": events})
        self.chain_cg.rmdir()
        if oom:
            return EXIT_OOM
        return code if code >= 0 else 128 - code


# ---------------------------------------------------------------- commands

def guarded_supervisor(args) -> Supervisor:
    require(sys.platform.startswith("linux"), "The W5 launcher runs on Linux only")
    plan, plan_sha = load_plan(args.plan)
    spec = chain_spec(plan, args.chain)
    frozen = plan["layout"]
    layout = compute_layout(read_topology(), frozen["cores_per_box"], frozen["offbox_cores_per_domain"],
                            len(frozen["boxes"]))
    detected = host_class()
    check_pins(plan, spec, layout, detected)
    throughput = None
    if args.qualification:
        require(args.max_wall_seconds and 0 < args.max_wall_seconds <= QUALIFICATION_MAX_WALL,
                "A qualification run needs --max-wall-seconds within 8 days")
    else:
        require(args.max_wall_seconds is None, "--max-wall-seconds is for qualification runs")
        ref = plan.get("throughput")
        require(ref is not None, "Plan names no box throughput record (COMPUTE-POLICY item 5)")
        require(sha256_file(ref["path"]) == ref["sha256"], "Throughput record hash changed")
        throughput = dict(require_throughput(json.loads(Path(ref["path"]).read_text()),
                                             throughput_binding(plan, spec)), sha256=ref["sha256"])
    return Supervisor(plan, plan_sha, args.chain, args.qualification, args.max_wall_seconds, layout, detected,
                      throughput)


def request(args, name: str) -> int:
    plan, _ = load_plan(args.plan)
    chain_spec(plan, args.chain)
    control = Path(plan["state_root"]) / args.chain / "control"
    require(control.exists(), f"Chain {args.chain} has no supervisor state")
    payload = json.loads(args.payload) if args.payload else {}
    if name == "pause.request":
        accepted = sorted(int(p.name.split(".")[1]) for p in control.glob("pause.*.accepted"))
        payload = dict(payload, seq=(accepted[-1] + 1) if accepted else 0, by="operator")
    write_json_atomic(control / name, payload)
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
    for name in ("pause", "resume", "kill", "ledger", "verify-ledger"):
        p = sub.add_parser(name)
        p.add_argument("--plan", required=True)
        p.add_argument("--chain", required=True)
        if name in ("pause", "resume"):
            p.add_argument("--payload", default=None)
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
        if args.command in ("pause", "resume", "kill"):
            return request(args, f"{args.command}.request")
        plan, _ = load_plan(args.plan)
        spec = chain_spec(plan, args.chain)
        ledger = Ledger(Path(plan["state_root"]) / args.chain / "ledger.jsonl")
        if args.command == "verify-ledger":
            print(json.dumps(ledger.summary(), indent=2))
            return 0
        slot = plan["layout"]["boxes"][spec["box_slot"]]
        cg = Cgroup(Path(plan["cgroup_root"]) / f"box{slot['slot']}" / "chain")
        print(json.dumps(ledger_value(ledger, cg), indent=2))
        return 0
    except Refusal as exc:
        print(f"refused: {exc}", file=sys.stderr)
        return EXIT_REFUSED


if __name__ == "__main__":
    raise SystemExit(main())
