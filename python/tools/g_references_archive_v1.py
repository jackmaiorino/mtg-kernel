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
# Per-step bounds (a walk of the old root took 11 min alone on 11-01).
STEP_SECONDS = {"baselines": 600, "pack": 3000, "copy": 3000, "prune": 900, "check": 900}
STARTUP_MARGIN_SECONDS = 120  # each step ends this far inside the resident guard's startup-idle bound
# The archive lease's bounds, which the launcher uses (lease_launch.py archive): the hard stop, and the resident
# guard's startup-idle bound (no progress.json on this lease) at the deadline less the recovery window. A Pod ready
# and staged, then a walk and a pack, end 30 to 46 min after creation; the copy and the prune must still fit.
LEASE_HOURS = 2.5
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
import hashlib, os, pathlib, re, tarfile, time
started = time.monotonic()
root, campaign, out = pathlib.Path(ARGS["root"]), pathlib.Path(ARGS["campaign"]), pathlib.Path(ARGS["out"])
need(not root.is_symlink() and str(root.resolve()) == str(root), "the superseded root is not canonical")
need(campaign.parent == root and campaign.is_dir() and not campaign.is_symlink(),
     "the campaign is not under the superseded root")
keep = set(ARGS["checkpoints"])
out.mkdir(parents=True, exist_ok=True)
included, listed = [], []
def fail(error):  # a folder that cannot be listed stops the archive (and so the prune)
    raise error
def in_time():
    need(time.monotonic() - started < ARGS["limit_seconds"], "the pack outlasted its bound")
class Timed:
    # a file read that checks the pack's bound at every chunk (hashing and the tar member alike)
    def __init__(self, stream):
        self.stream = stream
    def read(self, size=-1):
        in_time()
        return self.stream.read(size if size and size > 0 else 1 << 20)
def timed_digest(path):
    value = hashlib.sha256()
    with open(path, "rb") as stream:
        reader = Timed(stream)
        for chunk in iter(lambda: reader.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()
with tarfile.open(out / "records.tar.gz", "w:gz", compresslevel=1) as tar:
    for folder, dirs, files in os.walk(root, onerror=fail):
        in_time()
        for name in dirs:
            need(not pathlib.Path(folder, name).is_symlink(), "a linked folder in the superseded root")
        dirs.sort()
        for name in sorted(files):
            in_time()
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
            entry = {"path": posix, "bytes": size}
            if payload:
                listed.append({**entry, "kind": "shard" if shard else "checkpoint"
                               if parts[:2] == ("campaign", "checkpoints") else "native"})
                continue
            sha256 = timed_digest(path)
            info = tarfile.TarInfo(posix)
            info.size, info.mtime, info.mode = size, int(path.stat().st_mtime), 0o644
            with open(path, "rb") as stream:
                tar.addfile(info, Timed(stream))
            need(path.stat().st_size == size, "a file changed while it was archived: " + str(path))
            included.append({**entry, "sha256": sha256})
kept = {item["path"] for item in included}
need(all(pathlib.Path(path).relative_to(root).as_posix() in kept for path in keep),
     "a final checkpoint is missing from the archive")
(out / "manifest.json").write_text(json.dumps({"schema": ARGS["schema"] + "/manifest", "root": str(root),
                                               "files": included}, indent=1), encoding="utf-8")
(out / "prune-listing.json").write_text(json.dumps({"schema": ARGS["schema"] + "/prune-listing", "root": str(root),
                                                    "files": listed}, indent=1), encoding="utf-8")
print(json.dumps({"files": {name: timed_digest(out / name) for name in ("records.tar.gz", "manifest.json",
                                                                          "prune-listing.json")},
                  "archive_bytes": (out / "records.tar.gz").stat().st_size,
                  "included": len(included), "included_bytes": sum(item["bytes"] for item in included),
                  "listed": len(listed), "listed_bytes": sum(item["bytes"] for item in listed),
                  "seconds": round(time.monotonic() - started, 1)}))
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

    def step(self, kind, program, args):
        self.in_bounds(kind)
        started = time.monotonic()
        result = self.pod.run(program, {**args, "schema": SCHEMA}, timeout=STEP_SECONDS[kind])
        self.result.setdefault("step_seconds", {})[kind] = round(time.monotonic() - started, 1)
        return result

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
                                              "limit_seconds": STEP_SECONDS["pack"] - 60})
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
        self.in_bounds("copy")
        started = time.monotonic()
        deadline = started + STEP_SECONDS["copy"]  # one bound for the whole copy, shared by its downloads
        for name in FILES:
            left = deadline - time.monotonic()
            require(left > 0, "the copy outlasted its bound")
            self.pod.get(f"{POD_OUT}/{name}", self.dest / name, timeout=left)
        self.result.setdefault("step_seconds", {})["copy"] = round(time.monotonic() - started, 1)
        flush_durable(self.dest, FILES)  # on disk before anything depends on these copies
        verified = verify_copy(self.dest, reported)
        write(self.dest / "VERIFIED.json", {"schema": SCHEMA + "/verified", "epoch": time.time(),
                                            "files": {name: pin(self.dest / name) for name in FILES}, **verified})
        flush_durable(self.dest, ["VERIFIED.json"])
        pruned = self.prune(reported)
        require(pruned["pruned"] is True, "the superseded root is still present")
        self.finish(verified, pruned)

    def prune_verified(self):
        """Resume after a lease that verified its copy but did not prune: re-verify the E: copy against VERIFIED.json
        and the manifest, then prune on a new Pod (a partly removed root included)."""
        require((self.dest / "VERIFIED.json").exists() and not (self.dest / "PRUNE.json").exists(),
                "a resumed prune needs a verified, unpruned archive destination")
        record = read(self.dest / "VERIFIED.json")
        require(record.get("schema") == SCHEMA + "/verified" and set(record.get("files", {})) == set(FILES),
                "VERIFIED.json is not this archive's verification record")
        verified = verify_copy(self.dest, {name: item["sha256"] for name, item in record["files"].items()})
        roots = {read(self.dest / name)["root"] for name in ("manifest.json", "prune-listing.json")}
        require(roots == {str(self.old)}, f"the verified archive is of {sorted(roots)}, not of {self.old}")
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
    if args.prune_verified:
        require((args.dest / "VERIFIED.json").exists() and not (args.dest / "PRUNE.json").exists(),
                "a resumed prune needs a verified, unpruned archive destination")
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
