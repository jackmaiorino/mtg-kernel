"""G(i) archive lease: archive and prune a superseded campaign root on the network volume (QUALIFICATION-RESTART).

Lead ruling 2026-11-01: before the q2 qualification blocks run, the old root (/workspace/g-references-v1) is archived
and pruned on its own short lease, so the restart runs with one accounting root. On the Pod, in this order, and any
failed check stops before the prune:

1. **Baselines.** The nine determinism baselines (``BASELINES``: G1 blocks 1 and 2, G2 to G8 block 1) are computed on
   ``IDENTITY_KEYS`` with the qualification tool's own ``checkpoint_identity``, and recorded.
2. **Copy.** Every file of the old root is packed (tar, gzip level 1) on the container disk with a manifest (path
   relative to the root, bytes, SHA256), except regenerable bulk, which a listing names (path, bytes): native trees,
   cold trajectory shards (``native-<k>.zip``) and checkpoints other than the nine final ones. Kept: each cold
   folder's ``archive.json``, G1 block 1's cold shards (a byte-level reference, lead ruling), the old leases'
   control files, and the staged tooling, runtime and inputs (``inputs/objects/<sha>/campaign.json`` is
   hash-cited). The root must be canonical, and a link or a folder that cannot be listed stops the archive.
3. **Verify.** The controller checks the destination's 60 GiB reserve and the archive's byte budget, copies the
   archive, manifest, listing and baselines to E:, flushes them to disk, and verifies every file's SHA256 and every
   archive member against the manifest.
4. **Prune.** Only then, and only if the Pod's copies still hash as verified, the old root is removed and its
   absence confirmed. ``PRUNE.json`` is the PRUNE manifest: the listing, the regeneration recipe of each kind of
   pruned file, the cold ``archive.json`` hashes and the ruling.

``--prune-verified`` resumes a lease that verified its copy but did not prune: on a new Pod it re-verifies the E:
copy against ``VERIFIED.json`` and the manifest, then prunes, a partly removed root included.

The lease is the G controller's (``g_references_cloud_v1.Cycle``) with the staged package's tooling, no campaign
driver and no recovery; its guards, hard stop and release are unchanged. No progress.json is written, so each Pod
step must end inside the resident guard's startup-idle bound as well as the lease's deadline.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import tarfile
import time
from pathlib import Path, PurePosixPath

import g_references_cloud_v1 as gcloud
import nine_deck_cloud_v1 as cloud
from nine_deck_cloud_v1 import pin, read, require, write

SCHEMA = "g-references-v1/superseded-archive"
BASELINES = [("G1", 1), ("G1", 2)] + [(run, 1) for run in ("G2", "G3", "G4", "G5", "G6", "G7", "G8")]
# Regenerable bulk, pruned without a copy: native trees (a "native" folder anywhere), cold trajectory shards
# (native-<k>.zip inside a cold folder) and checkpoints other than the named final ones.
PAYLOAD_FOLDERS = ("native",)
PAYLOAD_SHARD = r"native-[0-9]+\.zip"
SHARD_FOLDERS = ("cold", "trials-cold")  # a shard is payload only inside a cold folder
KEEP_SHARDS = ("campaign/cold/G1/b001-",)  # G1 block 1's shards stay as a byte-level reference (lead ruling)
PACK_RULES = {"payload_folders": list(PAYLOAD_FOLDERS), "payload_shard": PAYLOAD_SHARD,
              "shard_folders": list(SHARD_FOLDERS), "keep_shards": list(KEEP_SHARDS)}
POD_OUT = "/root/superseded-archive"  # the container disk: the archive never adds to the volume
FILES = ("records.tar.gz", "manifest.json", "prune-listing.json", "baselines.json")
# Per-step bounds. Measured on lease 20261101-2012: one walk of the old root 11 min (11-01 morning); the first pack
# read each file twice, one at a time, at about 7 MB/s, and had 3.5 GB of archive after 31 min (about 6.8 GB of
# files read) without finishing. The pack now reads each file once, small files through a prefetch pool; its bound
# keeps the single-stream rate as the worst case for about 15 GB of records with margin.
STEP_SECONDS = {"baselines": 900, "pack": 5400, "copy": 3600, "prune": 1800, "check": 1800}
STARTUP_MARGIN_SECONDS = 120  # each step ends this far inside the resident guard's startup-idle bound
POLL_SECONDS = 30  # a detached Pod step's status poll
PREFETCH_WORKERS = 8  # small files read ahead in parallel (the network volume is latency bound per file)
PREFETCH_SMALL_BYTES = 64 * 1024**2  # files below this are read whole by the pool; larger ones are streamed
# The archive lease's bounds, which the launcher uses (lease_launch.py archive): the hard stop, and the resident
# guard's startup-idle bound (no progress.json on this lease) at the deadline less the recovery window. Pod ready and
# staged (about 10 min), then the baselines, pack, copy and prune at their bounds, must fit.
LEASE_HOURS = 4.0
LEASE_RECOVERY_SECONDS = 600
LEASE_STARTUP_IDLE_SECONDS = int(LEASE_HOURS * 3600) - LEASE_RECOVERY_SECONDS
RESERVE_BYTES = 60 * 1024**3  # ARTIFACT-LAW: the destination volume keeps 60 GiB free
RULING = ("lead ruling 2026-11-01 (collab LANES/g-references-20261028/RUN-MANIFEST.json lead_rulings; "
          "QUALIFICATION-RESTART.md): the superseded root is archived and pruned on its own lease; its trajectory "
          "shards are pruned on that ruling, not on a regeneration receipt (none exists yet for this trainer on the "
          "Pod); G1 block 1's shards are kept as a byte-level reference")
RECIPES = {
    "native": "deterministic replay of the block from its frozen block-start checkpoint and seeds by the G driver "
              "(DESIGN section 4); the restart's root q2 replays blocks 1 and 2",
    "shard": "the native tree of the same replay, archived by native_expanded_dispatch_v1's archive step; each "
             "folder's archive.json, kept, names every shard's SHA256",
    "checkpoint": "deterministic replay as for native trees; the nine final checkpoints are kept",
}

POD_BASELINES = '''#pod:archive-baselines
import pathlib, sys
sys.path.insert(0, ARGS["tools"])
import g_references_qualify_v1 as qualify
old = pathlib.Path(ARGS["campaign"])
rows = []
for run, block in ARGS["pairs"]:
    path = old / "state" / (run + ".json")
    entry = json.loads(path.read_text(encoding="utf-8"))["blocks"].get(str(block)) if path.exists() else None
    item = (entry or {}).get("final_checkpoint")
    row = {"run": run, "block": block, "final_checkpoint": item}
    if item is None:
        row["identity"] = None
    else:
        row["identity"] = {key: value for key, value in qualify.checkpoint_identity(item).items()
                           if key in qualify.IDENTITY_KEYS}
    rows.append(row)
out = pathlib.Path(ARGS["out"])
out.mkdir(parents=True, exist_ok=True)
value = {"schema": ARGS["schema"] + "/baselines", "campaign": str(old), "identity_keys": list(qualify.IDENTITY_KEYS),
         "baselines": rows}
(out / "baselines.json").write_text(json.dumps(value, indent=1, sort_keys=True), encoding="utf-8")
print(json.dumps({"baselines": rows, "sha256": digest(out / "baselines.json")}))
'''

POD_PACK = '''#pod:archive-pack
import collections, concurrent.futures, hashlib, io, os, pathlib, re, tarfile, time
started = time.monotonic()
root, campaign, out = pathlib.Path(ARGS["root"]), pathlib.Path(ARGS["campaign"]), pathlib.Path(ARGS["out"])
need(not root.is_symlink() and str(root.resolve()) == str(root), "the superseded root is not canonical")
need(campaign.parent == root and campaign.is_dir() and not campaign.is_symlink(),
     "the campaign is not under the superseded root")
keep = set(ARGS["checkpoints"])
out.mkdir(parents=True, exist_ok=True)
included, listed, records = [], [], []
def fail(error):  # a folder that cannot be listed stops the archive (and so the prune)
    raise error
def stop_now(message):  # ends the whole process, prefetch readers included (a stalled read must not hold it)
    import sys
    sys.stderr.write(message + "\\n")
    sys.stderr.flush()
    os._exit(3)
def left():
    return ARGS["limit_seconds"] - (time.monotonic() - started)
def in_time():
    if left() <= 0:
        stop_now("the pack outlasted its bound")
last = [0.0]
def progress(phase):
    if time.monotonic() - last[0] >= 10:
        last[0] = time.monotonic()
        partial = out / "pack-progress.json.partial"
        partial.write_text(json.dumps({"phase": phase, "seconds": round(time.monotonic() - started, 1),
                                       "listed": len(listed), "records": len(records), "included": len(included),
                                       "included_bytes": sum(item["bytes"] for item in included)}))
        os.replace(partial, out / "pack-progress.json")
# 1. The walk: every file is a record (archived) or regenerable bulk (listed).
for folder, dirs, files in os.walk(root, onerror=fail):
    in_time()
    progress("walk")
    for name in dirs:
        need(not pathlib.Path(folder, name).is_symlink(), "a linked folder in the superseded root")
    dirs.sort()
    for name in sorted(files):
        path = pathlib.Path(folder, name)
        need(not path.is_symlink(), "a link in the superseded root: " + str(path))
        size = path.stat().st_size
        relative = path.relative_to(root)
        parts, posix = relative.parts, relative.as_posix()
        shard = re.fullmatch(ARGS["payload_shard"], name) is not None \\
            and any(part in ARGS["shard_folders"] for part in parts[:-1]) \\
            and not any(posix.startswith(prefix) for prefix in ARGS["keep_shards"])
        payload = (any(part in ARGS["payload_folders"] for part in parts[:-1]) or shard
                   or parts[:2] == ("campaign", "checkpoints") and str(path) not in keep)
        if payload:
            listed.append({"path": posix, "bytes": size, "kind": "shard" if shard else "checkpoint"
                           if parts[:2] == ("campaign", "checkpoints") else "native"})
        else:
            records.append((path, posix, size, path.stat().st_mtime_ns))
# 2. The archive: each record read once, hashed as it is written; small files read ahead by a pool.
class Hashing:
    def __init__(self, stream):
        self.stream, self.value, self.count = stream, hashlib.sha256(), 0
    def read(self, size=-1):
        in_time()
        data = self.stream.read(size if size and size > 0 else 1 << 20)
        self.value.update(data)
        self.count += len(data)
        return data
def whole(path):
    chunks = []
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            in_time()
            chunks.append(chunk)
    return b"".join(chunks)
def unchanged(path, size, mtime_ns):
    seen = path.stat()
    need(seen.st_size == size and seen.st_mtime_ns == mtime_ns, "a file changed while it was archived: " + str(path))
small = ARGS["small_bytes"]
with tarfile.open(out / "records.tar.gz", "w:gz", compresslevel=1) as tar, \\
        concurrent.futures.ThreadPoolExecutor(ARGS["workers"]) as pool:
    ahead = collections.deque()
    position = 0
    def refill():
        global position
        while len(ahead) < 4 * ARGS["workers"] and position < len(records):
            path, posix, size, mtime_ns = records[position]
            ahead.append((path, posix, size, mtime_ns, pool.submit(whole, path) if size < small else None))
            position += 1
    refill()
    while ahead:
        in_time()
        progress("archive")
        path, posix, size, mtime_ns, future = ahead.popleft()
        refill()
        info = tarfile.TarInfo(posix)
        info.size, info.mtime, info.mode = size, mtime_ns // 10**9, 0o644
        if future is not None:
            try:
                data = future.result(timeout=max(1.0, left()))
            except concurrent.futures.TimeoutError:
                stop_now("the pack outlasted its bound")
            need(len(data) == size, "a file changed while it was archived: " + str(path))
            tar.addfile(info, io.BytesIO(data))
            sha256 = hashlib.sha256(data).hexdigest()
        else:
            with open(path, "rb") as stream:
                reader = Hashing(stream)
                tar.addfile(info, reader)
            need(reader.count == size, "a file changed while it was archived: " + str(path))
            sha256 = reader.value.hexdigest()
        unchanged(path, size, mtime_ns)  # after its bytes went into the archive: an append or rewrite stops the pack
        included.append({"path": posix, "bytes": size, "sha256": sha256})
kept = {item["path"] for item in included}
need(all(pathlib.Path(path).relative_to(root).as_posix() in kept for path in keep),
     "a final checkpoint is missing from the archive")
(out / "manifest.json").write_text(json.dumps({"schema": ARGS["schema"] + "/manifest", "root": str(root),
                                               "files": included}, indent=1), encoding="utf-8")
(out / "prune-listing.json").write_text(json.dumps({"schema": ARGS["schema"] + "/prune-listing", "root": str(root),
                                                    "files": listed}, indent=1), encoding="utf-8")
print(json.dumps({"files": {name: digest(out / name) for name in ("records.tar.gz", "manifest.json",
                                                                    "prune-listing.json")},
                  "archive_bytes": (out / "records.tar.gz").stat().st_size,
                  "included": len(included), "included_bytes": sum(item["bytes"] for item in included),
                  "listed": len(listed), "listed_bytes": sum(item["bytes"] for item in listed),
                  "seconds": round(time.monotonic() - started, 1)}))
'''

# A long Pod step runs detached from the controller's SSH session: started in its own session with its output,
# exit code and progress in files under POD_OUT, then polled by short SSH calls. A controller network interruption
# (lease 20261101-2012) no longer ends the step; the step's own bound still does.
POD_DETACH_START = '''#pod:archive-detach-start
import os, pathlib, shlex, subprocess, time
out = pathlib.Path(ARGS["out"])
out.mkdir(parents=True, exist_ok=True)
name = ARGS["name"]
try:  # one claim per step and Pod, created exclusively: a retried start never launches a second copy
    claim = os.open(out / (name + ".started.json"), os.O_CREAT | os.O_EXCL | os.O_WRONLY)
except FileExistsError:
    raise SystemExit("the " + name + " step was already started on this Pod")
with os.fdopen(claim, "w") as stream:
    stream.write(json.dumps({"state": "launching", "epoch": time.time()}))
script = out / (name + ".py")
script.write_text(ARGS["code"])
q = shlex.quote
identity = q(str(out / (name + ".identity")))
# The wrapper shell records its own PID and start ticks before the step runs, so the step is traceable even if this
# starter dies right after the launch.
command = ("echo $$ $(cut -d' ' -f22 /proc/$$/stat) > %s.partial && mv %s.partial %s; "
           "python3 -B %s > %s 2> %s; echo $? > %s && mv %s %s" %
           (identity, identity, identity, q(str(script)), q(str(out / (name + ".stdout"))),
            q(str(out / (name + ".stderr"))), q(str(out / (name + ".exit.partial"))),
            q(str(out / (name + ".exit.partial"))), q(str(out / (name + ".exit")))))
subprocess.Popen(["sh", "-c", command], start_new_session=True, stdin=subprocess.DEVNULL,
                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print(json.dumps({"started": True}))
'''

POD_DETACH_POLL = '''#pod:archive-detach-poll
import pathlib, time
out, name = pathlib.Path(ARGS["out"]), ARGS["name"]
started, exited = out / (name + ".started.json"), out / (name + ".exit")
def tail(path):
    try:
        return path.read_text(errors="replace")[-1500:]
    except OSError:
        return ""
progress = None
if ARGS.get("progress"):
    try:
        progress = json.loads((out / ARGS["progress"]).read_text())
    except (OSError, ValueError):
        progress = None
if not started.exists():
    value = {"state": "absent"}
elif exited.exists():
    code = int(exited.read_text().strip() or "1")
    if code == 0:
        lines = [line for line in (out / (name + ".stdout")).read_text().splitlines() if line.strip()]
        value = {"state": "done", "result": json.loads(lines[-1])}
    else:
        value = {"state": "failed", "exit": code, "error": tail(out / (name + ".stderr"))}
else:
    identity = out / (name + ".identity")
    if not identity.exists():  # claimed, wrapper not yet up
        try:
            claimed = json.loads(started.read_text())["epoch"]
        except (OSError, ValueError, KeyError):
            claimed = started.stat().st_mtime
        stale = time.time() - claimed > 60
        value = {"state": "failed", "error": "the step never started"} if stale else {"state": "running"}
    elif alive(*[int(field) for field in identity.read_text().split()[:2]]):
        value = {"state": "running"}
    else:
        value = {"state": "failed", "error": "the step ended without an exit record: " + tail(out / (name + ".stderr"))}
value["progress"] = progress
print(json.dumps(value))
'''

POD_REMAINING = '''#pod:archive-remaining
import os, pathlib
# A resumed prune removes only what the verified archive accounts for: every file still present is either archived
# (the manifest) or listed for the prune.
root = pathlib.Path(ARGS["root"])
known, unknown = set(ARGS["known"]), []
if root.exists():
    need(not root.is_symlink() and str(root.resolve()) == str(root), "the superseded root is not canonical")
    def fail(error):
        raise error
    for folder, dirs, files in os.walk(root, onerror=fail):
        for name in dirs:
            need(not pathlib.Path(folder, name).is_symlink(), "a linked folder in the superseded root")
        for name in files:
            relative = pathlib.Path(folder, name).relative_to(root).as_posix()
            if relative not in known:
                unknown.append(relative)
print(json.dumps({"present": root.exists(), "unknown": sorted(unknown)[:50], "unknown_count": len(unknown)}))
'''

POD_PRUNE = '''#pod:archive-prune
import pathlib, shutil
root, out = pathlib.Path(ARGS["root"]), pathlib.Path(ARGS["out"])
need(str(root) == ARGS["expected_root"] and root != pathlib.Path(ARGS["keep_root"]) and
     root.parent == pathlib.Path(ARGS["workspace"]), "not the superseded root")
need(not root.is_symlink(), "the superseded root is a link")
for name, expected in ARGS["verified"].items():  # this lease's own Pod copies (none on a resumed prune)
    need(digest(out / name) == expected, "the Pod copy of " + name + " differs from the verified one")
present = root.exists()
if present:
    need(str(root.resolve()) == str(root), "the superseded root is not canonical")
    shutil.rmtree(root)
print(json.dumps({"pruned": not root.exists(), "was_present": present}))
'''


def sha256_file(path) -> str:
    value = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()


def flush_durable(folder: Path, names) -> None:
    """Flush each copied file, then its folder and the folder's parents where the platform allows, to stable
    storage: a verified copy must survive a crash of this host before the Pod's copy is pruned."""
    for name in names:
        with open(Path(folder) / name, "rb+") as stream:
            os.fsync(stream.fileno())
    if hasattr(os, "O_DIRECTORY"):  # POSIX: the folder's entries, and each parent's (the folder may be new), too
        for each in (Path(folder).resolve(), *Path(folder).resolve().parents):
            handle = os.open(each, os.O_RDONLY | os.O_DIRECTORY)
            try:
                os.fsync(handle)
            finally:
                os.close(handle)


def verify_copy(dest: Path, reported: dict) -> dict:
    """Every copied file hashes as reported, and every archive member matches the manifest (name, bytes, SHA256)
    with nothing missing or extra. Raises on any difference."""
    for name, expected in reported.items():
        require(sha256_file(dest / name) == expected, f"{name} differs from the reported copy")
    manifest = read(dest / "manifest.json")
    expected = {item["path"]: item for item in manifest["files"]}  # paths relative to the root, POSIX
    seen = set()
    with tarfile.open(dest / "records.tar.gz", "r:gz") as tar:
        for member in tar:
            require(member.isfile() and member.name in expected and member.name not in seen,
                    f"unexpected archive member {member.name}")
            value, size = hashlib.sha256(), 0
            stream = tar.extractfile(member)
            for chunk in iter(lambda: stream.read(1 << 20), b""):
                value.update(chunk)
                size += len(chunk)
            item = expected[member.name]
            require(size == item["bytes"] and value.hexdigest() == item["sha256"],
                    f"archive member {member.name} differs from the manifest")
            seen.add(member.name)
    require(seen == set(expected), "archive members missing: " + ", ".join(sorted(set(expected) - seen)[:5]))
    return {"members": len(seen), "bytes": sum(item["bytes"] for item in expected.values()),
            "archive_bytes": (dest / "records.tar.gz").stat().st_size}


def prune_manifest(dest: Path, old: PurePosixPath, verified: dict, pruned: dict) -> dict:
    """PRUNE.json (ARTIFACT-LAW clause 3): the listing, what each kind of pruned file is regenerated from, the kept
    cold archive.json hashes that name the pruned shards, and the ruling."""
    listing = read(dest / "prune-listing.json")
    manifest = read(dest / "manifest.json")
    kinds = {}
    for item in listing["files"]:
        kind = kinds.setdefault(item["kind"], {"files": 0, "bytes": 0})
        kind["files"] += 1
        kind["bytes"] += item["bytes"]
    return {"schema": SCHEMA + "/prune", "epoch": time.time(), "root": str(old),
            "listing": pin(dest / "prune-listing.json"), "files": len(listing["files"]),
            "bytes": sum(item["bytes"] for item in listing["files"]), "kinds": kinds, "recipes": RECIPES,
            "cold_archive_records": {item["path"]: item["sha256"] for item in manifest["files"]
                                     if PurePosixPath(item["path"]).name == "archive.json"},
            "archived": verified, "pod": pruned, "ruling": RULING}


def reverify(dest: Path, old) -> dict:
    """A resumed prune's E: checks: a verified, unpruned destination whose VERIFIED.json is this archive's record,
    every file and member still as verified, and an archive of ``old`` itself."""
    require((dest / "VERIFIED.json").exists() and not (dest / "PRUNE.json").exists(),
            "a resumed prune needs a verified, unpruned archive destination")
    record = read(dest / "VERIFIED.json")
    require(record.get("schema") == SCHEMA + "/verified" and set(record.get("files", {})) == set(FILES),
            "VERIFIED.json is not this archive's verification record")
    verified = verify_copy(dest, {name: item["sha256"] for name, item in record["files"].items()})
    roots = {read(dest / name)["root"] for name in ("manifest.json", "prune-listing.json")}
    require(roots == {str(old)}, f"the verified archive is of {sorted(roots)}, not of {old}")
    return verified


class ArchiveCycle(gcloud.Cycle):
    """The G controller's lease with the archive steps in place of the campaign driver."""

    def __init__(self, *args, superseded: str, dest: Path, budget_bytes: int, resume: bool = False, **kwargs):
        super().__init__(*args, **kwargs)
        self.old = cloud.WORKSPACE / superseded
        require(self.old != self.base and self.old.parent == cloud.WORKSPACE, "the superseded root is another root")
        self.dest, self.budget, self.resume = Path(dest), budget_bytes, resume
        self.result.update(superseded_root=str(self.old), archive_dest=str(self.dest), resumed_prune=resume)

    def launch(self):
        require(time.time() - self.lease["created_epoch"] < self.lease["startup_idle_seconds"] - 60,
                "startup outlasted the guards' startup idle bound")

    def watch(self):
        self.prune_verified() if self.resume else self.archive()
        self.ended = self.result["complete"] = True

    def recover(self):
        self.result["recovery"] = "not applicable: the archive lease has no campaign driver"

    def in_bounds(self, kind):
        """A step of ``kind`` ends inside the lease's deadline and the resident guard's startup-idle bound (this
        lease writes no progress.json)."""
        end = time.time() + STEP_SECONDS[kind]
        require(end < self.lease["deadline_epoch"] - self.lease["recovery_seconds"],
                f"too little lease time left for the {kind} step")
        require(end < self.lease["created_epoch"] + self.lease["startup_idle_seconds"] - STARTUP_MARGIN_SECONDS,
                f"the {kind} step could outlast the resident guard's startup-idle bound")

    def step(self, kind, program, args, progress=None):
        """Run a Pod step detached (POD_DETACH_START) and poll it (POD_DETACH_POLL) until it ends or its bound
        passes. A poll or start the network interrupts is retried at the next poll; the step keeps running on the
        Pod meanwhile, and a start already made is never repeated."""
        self.in_bounds(kind)
        started = time.monotonic()
        deadline = started + STEP_SECONDS[kind]
        code = cloud.PRELUDE % json.dumps({**args, "schema": SCHEMA}) + program
        poll = {"out": POD_OUT, "name": kind, "progress": progress}
        launched, errors = False, 0

        def left():
            return deadline - time.monotonic()
        while True:
            require(left() > 0, f"the {kind} step outlasted its bound")
            try:  # one attempt per call, each inside what remains of the step's bound
                state = self.pod.run(POD_DETACH_POLL, poll, timeout=max(1, min(60, left())))
                if state["state"] == "absent" and not launched:
                    require(left() > 0, f"the {kind} step outlasted its bound")
                    self.pod.run(POD_DETACH_START, {"out": POD_OUT, "name": kind, "code": code, "args": args},
                                 timeout=max(1, min(60, left())))
                    launched = True
                    continue
                launched = True
                if state.get("progress"):
                    write(self.control / f"archive-{kind}-progress.json", state["progress"], replace=True)
                if state["state"] == "done":
                    self.result.setdefault("step_seconds", {})[kind] = round(time.monotonic() - started, 1)
                    return state["result"]
                require(state["state"] in ("running", "absent"), f"the {kind} step failed: {state.get('error')}")
            except cloud.PodUnavailable as error:  # the network or the Pod's SSH: look again at the next poll
                errors += 1
                self.result.setdefault("poll_errors", {})[kind] = errors
                self.result["last_poll_error"] = str(error)[-300:]
            self.sleep(max(0, min(POLL_SECONDS, left())))

    def prune(self, verified_pod_copies):
        return self.step("prune", POD_PRUNE, {"root": str(self.old), "expected_root": str(self.old),
                                              "keep_root": str(self.base), "workspace": str(cloud.WORKSPACE),
                                              "out": POD_OUT, "verified": verified_pod_copies})

    def archive(self):
        require(not self.dest.exists(), "fresh archive destination required")
        campaign = self.old / "campaign"
        tools = str(PurePosixPath(self.manifest["tooling"]["root"]) / "python/tools")
        baselines = self.step("baselines", POD_BASELINES, {"tools": tools, "campaign": str(campaign),
                                                           "pairs": BASELINES, "out": POD_OUT})
        write(self.control / "archive-baselines.json", baselines)
        missing = [f"{row['run']} block {row['block']}" for row in baselines["baselines"] if row["identity"] is None]
        require(not missing, "determinism baselines missing: " + ", ".join(missing))
        checkpoints = [row["final_checkpoint"]["path"] for row in baselines["baselines"]]
        packed = self.step("pack", POD_PACK, {"root": str(self.old), "campaign": str(campaign),
                                              "checkpoints": checkpoints, **PACK_RULES, "out": POD_OUT,
                                              "limit_seconds": STEP_SECONDS["pack"] - 120, "workers": PREFETCH_WORKERS,
                                              "small_bytes": PREFETCH_SMALL_BYTES},
                           progress="pack-progress.json")
        write(self.control / "archive-packed.json", packed)
        require(packed["archive_bytes"] <= self.budget,
                f"the archive ({packed['archive_bytes']} B) exceeds its byte budget ({self.budget} B)")
        anchor = self.dest.parent
        while not anchor.exists():
            require(anchor != anchor.parent, f"the destination's drive is missing: {self.dest}")
            anchor = anchor.parent
        free = shutil.disk_usage(anchor).free
        require(free - packed["archive_bytes"] - packed["included_bytes"] >= RESERVE_BYTES,
                "the destination would fall below its 60 GiB reserve")
        reported = {**packed["files"], "baselines.json": baselines["sha256"]}
        self.dest.mkdir(parents=True)
        self.copy(reported)
        flush_durable(self.dest, FILES)  # on disk before anything depends on these copies
        verified = verify_copy(self.dest, reported)
        write(self.dest / "VERIFIED.json", {"schema": SCHEMA + "/verified", "epoch": time.time(),
                                            "files": {name: pin(self.dest / name) for name in FILES}, **verified})
        flush_durable(self.dest, ["VERIFIED.json"])
        pruned = self.prune(reported)
        require(pruned["pruned"] is True, "the superseded root is still present")
        self.finish(verified, pruned)

    def copy(self, reported):
        """Copy each file from the Pod, one bound for the whole copy. A file whose download the network interrupts,
        or whose bytes differ from the Pod's reported hash, is fetched again; a file already copied and matching is
        not."""
        self.in_bounds("copy")
        started = time.monotonic()
        deadline = started + STEP_SECONDS["copy"]
        for name in FILES:
            target = self.dest / name
            while not (target.exists() and sha256_file(target) == reported[name]):
                left = deadline - time.monotonic()
                require(left > 0, "the copy outlasted its bound")
                try:
                    self.pod.get(f"{POD_OUT}/{name}", target, timeout=left)
                except Exception as error:  # noqa: BLE001 - scp or the network: try again inside the bound
                    errors = self.result.setdefault("copy_errors", 0) + 1
                    self.result.update(copy_errors=errors, last_copy_error=str(error)[-300:])
                    require(time.monotonic() < deadline, "the copy outlasted its bound")
                    self.sleep(POLL_SECONDS)
        self.result.setdefault("step_seconds", {})["copy"] = round(time.monotonic() - started, 1)

    def prune_verified(self):
        """Resume after a lease that verified its copy but did not prune: re-verify the E: copy against VERIFIED.json
        and the manifest, then prune on a new Pod (a partly removed root included)."""
        verified = reverify(self.dest, self.old)  # again here: the copy must still match when the Pod is up
        known = [item["path"] for name in ("manifest.json", "prune-listing.json")
                 for item in read(self.dest / name)["files"]]
        remaining = self.step("check", POD_REMAINING, {"root": str(self.old), "known": known})
        require(remaining["unknown_count"] == 0,
                f"{remaining['unknown_count']} files under the root are in neither the archive nor the listing, "
                f"for example {remaining['unknown'][:3]}")
        pruned = self.prune({})
        require(pruned["pruned"] is True, "the superseded root is still present")
        self.finish(verified, pruned)

    def finish(self, verified, pruned):
        write(self.dest / "PRUNE.json", prune_manifest(self.dest, self.old, verified, pruned))
        flush_durable(self.dest, ["PRUNE.json"])
        self.result["archived"] = verified
        self.result["pruned"] = True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("op", choices=("plan", "execute"))
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--control", type=Path, required=True)
    parser.add_argument("--external-guard", type=Path, default=cloud.EXTERNAL_GUARD)
    parser.add_argument("--leases", type=Path, required=True)
    parser.add_argument("--volume-gb", type=int, required=True)
    parser.add_argument("--volume-created-epoch", type=float, required=True)
    parser.add_argument("--superseded", required=True, help="the superseded campaign root's name")
    parser.add_argument("--dest", type=Path, required=True)
    parser.add_argument("--budget-bytes", type=int, required=True, help="the compressed archive's byte budget")
    parser.add_argument("--prune-verified", action="store_true",
                        help="resume: re-verify an existing verified destination, then prune")
    parser.add_argument("--ssh-key", type=Path)
    parser.add_argument("--execute", action="store_true", help="allocate the Pod (paid compute)")
    args = parser.parse_args(argv)
    summary = gcloud.ledger(args.leases, args.volume_gb, args.volume_created_epoch)
    gcloud.verified_package(args.package)
    gcloud.require_within_caps(summary, read(args.control / "lease" / "lease.json"), args.volume_gb)
    if args.prune_verified:  # refuse a bad destination before any Pod exists
        reverify(args.dest, cloud.WORKSPACE / args.superseded)
    else:
        require(not args.dest.exists(), "fresh archive destination required")
    planned = cloud.plan(args.package, args.control, args.external_guard)
    if args.op == "plan" or not args.execute:
        print(json.dumps(planned | {"ledger": summary}))
        return 0
    require(args.ssh_key is not None and Path(args.ssh_key).is_file(), "SSH key unavailable")
    write(Path(args.control) / "plan.json", planned | {"ledger": summary})
    write(Path(args.control) / "controller-inputs.json", {"package": pin(Path(args.package) / "package.json"),
                                                          "controller": pin(__file__), "recipe": pin(gcloud.__file__),
                                                          "base_controller": pin(cloud.__file__),
                                                          "external_guard": pin(args.external_guard)})
    key = cloud.api_key()
    os.environ["RUNPOD_API_KEY"] = key
    result = ArchiveCycle(args.package, args.control, args.ssh_key, args.external_guard, key, False, None, None, None,
                          cloud.subprocess.run, cloud.spawn_detached, time.sleep, superseded=args.superseded,
                          dest=args.dest, budget_bytes=args.budget_bytes, resume=args.prune_verified).run()
    print(json.dumps(result))
    return 0 if result["complete"] and result.get("pruned") and result["release_confirmed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
