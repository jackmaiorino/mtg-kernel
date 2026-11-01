"""G(i) archive lease: archive and prune a superseded campaign root on the network volume (QUALIFICATION-RESTART).

Lead ruling 2026-11-01: before the q2 qualification blocks run, the old root (/workspace/g-references-v1) is archived
and pruned on its own short lease, so the restart runs with one accounting root. On the Pod, in this order, and any
failed check stops before the prune:

1. **Baselines.** The nine determinism baselines (``BASELINES``: G1 blocks 1 and 2, G2 to G8 block 1) are computed on
   ``IDENTITY_KEYS`` with the qualification tool's own ``checkpoint_identity``, and recorded.
2. **Copy.** The old campaign's records and its nine final checkpoints are packed (gzip) on the container disk with
   a manifest (path, bytes, SHA256 per file). A listing names every other file (path, bytes): the cold trajectory
   archives, native trees, other checkpoints and the staged tooling, runtime and inputs. These are regenerable by
   deterministic replay or from the packages, so they are pruned, not copied.
3. **Verify.** The controller copies the archive, manifest, listing and baselines to the destination on E: and
   verifies every file's SHA256 and every archive member against the manifest.
4. **Prune.** Only then, and only if the Pod's copies still hash as verified, the old root is removed and its
   absence confirmed. The listing is the PRUNE manifest.

The lease is the G controller's (``g_references_cloud_v1.Cycle``) with the staged package's tooling, no campaign
driver and no recovery; its guards, hard stop and release are unchanged.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import tarfile
import time
from pathlib import Path, PurePosixPath

import g_references_cloud_v1 as gcloud
import nine_deck_cloud_v1 as cloud
from nine_deck_cloud_v1 import pin, read, require, write

SCHEMA = "g-references-v1/superseded-archive"
BASELINES = [("G1", 1), ("G1", 2)] + [(run, 1) for run in ("G2", "G3", "G4", "G5", "G6", "G7", "G8")]
EXCLUDED_PARTS = ("cold", "trials-cold", "native")  # trajectory archives and native trees: regenerable
POD_OUT = "/root/superseded-archive"  # the container disk: the archive never adds to the volume
FILES = ("records.tar.gz", "manifest.json", "prune-listing.json", "baselines.json")
STEP_SECONDS = 3000  # a Pod step's bound (a walk of the old root took 11 min alone)

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
import hashlib, io, os, pathlib, tarfile
root, campaign, out = pathlib.Path(ARGS["root"]), pathlib.Path(ARGS["campaign"]), pathlib.Path(ARGS["out"])
need(campaign.parent == root and campaign.is_dir(), "the campaign is not under the superseded root")
keep = set(ARGS["checkpoints"])
out.mkdir(parents=True, exist_ok=True)
included, listed = [], []
with tarfile.open(out / "records.tar.gz", "w:gz", compresslevel=6) as tar:
    for folder, dirs, files in os.walk(root):
        dirs.sort()
        for name in sorted(files):
            path = pathlib.Path(folder, name)
            need(not path.is_symlink(), "a link in the superseded root: " + str(path))
            size = path.stat().st_size
            relative = path.relative_to(root)
            inside, parts = path.is_relative_to(campaign), relative.parts
            # Records: the campaign tree without trajectory archives, native trees and checkpoints, plus the named
            # final checkpoints, plus the old leases' Pod control files other than packed archives.
            record = inside and not any(part in ARGS["excluded"] for part in parts) and parts[1:2] != ("checkpoints",)
            record = record or inside and str(path) in keep
            record = record or parts[0] == "control" and not name.endswith(".tar.gz")
            if not record:
                listed.append({"path": str(path), "bytes": size})
                continue
            value, data = hashlib.sha256(), path.read_bytes()
            value.update(data)
            included.append({"path": str(path), "bytes": len(data), "sha256": value.hexdigest()})
            info = tarfile.TarInfo(relative.as_posix())
            info.size, info.mtime, info.mode = len(data), int(path.stat().st_mtime), 0o644
            tar.addfile(info, io.BytesIO(data))
need(keep <= {item["path"] for item in included}, "a final checkpoint is missing from the archive")
(out / "manifest.json").write_text(json.dumps({"schema": ARGS["schema"] + "/manifest", "root": str(root),
                                               "files": included}, indent=1), encoding="utf-8")
(out / "prune-listing.json").write_text(json.dumps({"schema": ARGS["schema"] + "/prune-listing", "root": str(root),
                                                    "files": listed}, indent=1), encoding="utf-8")
print(json.dumps({"files": {name: digest(out / name) for name in ("records.tar.gz", "manifest.json",
                                                                    "prune-listing.json")},
                  "included": len(included), "included_bytes": sum(item["bytes"] for item in included),
                  "listed": len(listed), "listed_bytes": sum(item["bytes"] for item in listed)}))
'''

POD_PRUNE = '''#pod:archive-prune
import pathlib, shutil
root, out = pathlib.Path(ARGS["root"]), pathlib.Path(ARGS["out"])
need(str(root) == ARGS["expected_root"] and root != pathlib.Path(ARGS["keep_root"]), "not the superseded root")
for name, expected in ARGS["verified"].items():
    need(digest(out / name) == expected, "the Pod copy of " + name + " differs from the verified one")
shutil.rmtree(root)
print(json.dumps({"pruned": not root.exists()}))
'''


def sha256_file(path) -> str:
    value = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            value.update(chunk)
    return value.hexdigest()


def verify_copy(dest: Path, reported: dict) -> dict:
    """Every copied file hashes as the Pod reported, and every archive member matches the manifest (name, bytes,
    SHA256) with nothing missing or extra. Raises on any difference."""
    for name, expected in reported.items():
        require(sha256_file(dest / name) == expected, f"{name} differs from the Pod's copy")
    manifest = read(dest / "manifest.json")
    root = PurePosixPath(manifest["root"])
    expected = {PurePosixPath(item["path"]).relative_to(root).as_posix(): item for item in manifest["files"]}
    seen = set()
    with tarfile.open(dest / "records.tar.gz", "r:gz") as tar:
        for member in tar:
            require(member.isfile() and member.name in expected and member.name not in seen,
                    f"unexpected archive member {member.name}")
            data = tar.extractfile(member).read()
            item = expected[member.name]
            require(len(data) == item["bytes"] and hashlib.sha256(data).hexdigest() == item["sha256"],
                    f"archive member {member.name} differs from the manifest")
            seen.add(member.name)
    require(seen == set(expected), "archive members missing: " + ", ".join(sorted(set(expected) - seen)[:5]))
    return {"members": len(seen), "bytes": sum(item["bytes"] for item in expected.values())}


class ArchiveCycle(gcloud.Cycle):
    """The G controller's lease with the archive steps in place of the campaign driver."""

    def __init__(self, *args, superseded: str, dest: Path, **kwargs):
        super().__init__(*args, **kwargs)
        self.old = cloud.WORKSPACE / superseded
        require(self.old != self.base and self.old.parent == cloud.WORKSPACE, "the superseded root is another root")
        self.dest = Path(dest)
        self.result.update(superseded_root=str(self.old), archive_dest=str(self.dest))

    def launch(self):
        require(time.time() - self.lease["created_epoch"] < self.lease["startup_idle_seconds"] - 60,
                "startup outlasted the guards' startup idle bound")

    def watch(self):
        self.archive()
        self.ended = self.result["complete"] = True

    def recover(self):
        self.result["recovery"] = "not applicable: the archive lease has no campaign driver"

    def step(self, program, args):
        require(time.time() + STEP_SECONDS < self.lease["deadline_epoch"] - self.lease["recovery_seconds"],
                "too little lease time left for an archive step")
        return self.pod.run(program, {**args, "schema": SCHEMA}, timeout=STEP_SECONDS)

    def archive(self):
        require(not self.dest.exists(), "fresh archive destination required")
        campaign = self.old / "campaign"
        tools = str(PurePosixPath(self.manifest["tooling"]["root"]) / "python/tools")
        baselines = self.step(POD_BASELINES, {"tools": tools, "campaign": str(campaign), "pairs": BASELINES,
                                              "out": POD_OUT})
        write(self.control / "archive-baselines.json", baselines)
        missing = [f"{row['run']} block {row['block']}" for row in baselines["baselines"] if row["identity"] is None]
        require(not missing, "determinism baselines missing: " + ", ".join(missing))
        checkpoints = [row["final_checkpoint"]["path"] for row in baselines["baselines"]]
        packed = self.step(POD_PACK, {"root": str(self.old), "campaign": str(campaign), "checkpoints": checkpoints,
                                      "excluded": list(EXCLUDED_PARTS), "out": POD_OUT})
        write(self.control / "archive-packed.json", packed)
        reported = {**packed["files"], "baselines.json": baselines["sha256"]}
        self.dest.mkdir(parents=True)
        for name in FILES:
            self.pod.get(f"{POD_OUT}/{name}", self.dest / name, timeout=STEP_SECONDS)
        verified = verify_copy(self.dest, reported)
        write(self.dest / "VERIFIED.json", {"schema": SCHEMA + "/verified", "epoch": time.time(),
                                            "files": {name: pin(self.dest / name) for name in FILES}, **verified})
        pruned = self.step(POD_PRUNE, {"root": str(self.old), "expected_root": str(self.old), "keep_root": str(self.base),
                                       "out": POD_OUT, "verified": reported})
        require(pruned["pruned"] is True, "the superseded root is still present")
        listing = read(self.dest / "prune-listing.json")
        write(self.dest / "PRUNE.json", {"schema": SCHEMA + "/prune", "epoch": time.time(), "root": str(self.old),
                                         "listing": pin(self.dest / "prune-listing.json"),
                                         "files": len(listing["files"]),
                                         "bytes": sum(item["bytes"] for item in listing["files"]),
                                         "archived": verified})
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
    parser.add_argument("--ssh-key", type=Path)
    parser.add_argument("--execute", action="store_true", help="allocate the Pod (paid compute)")
    args = parser.parse_args(argv)
    summary = gcloud.ledger(args.leases, args.volume_gb, args.volume_created_epoch)
    gcloud.verified_package(args.package)
    gcloud.require_within_caps(summary, read(args.control / "lease" / "lease.json"), args.volume_gb)
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
                          cloud.subprocess.run, cloud.spawn_detached, time.sleep,
                          superseded=args.superseded, dest=args.dest).run()
    print(json.dumps(result))
    return 0 if result["complete"] and result.get("pruned") and result["release_confirmed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
