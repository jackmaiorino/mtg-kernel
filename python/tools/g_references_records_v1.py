"""G(i) record copy-back (v1, collab LANES/g-references-20261028/AMENDMENT-2.md item 2).

The campaign driver seals each finished block past qualification
(``state/seals/<run>-<label>.json``: every file's path, bytes and SHA256 on its
raw bytes). This tool runs on Jack's PC beside a lease and, for each sealed block
the volume still holds, oldest first:

1. streams the seal's files from the Pod as one tar over ssh into
   ``<dest>/<run>/<label>.tar.zst.partial`` (zstd level 10, Python's
   ``compression.zstd``), with a copy of the seal beside it;
2. verifies the archive by decompressing it: its members are exactly the seal's
   files, each with the seal's bytes and SHA256. Hashes stay on the raw bytes, so
   the evidence is unchanged and a replay verifies after decompression;
3. renames it into place and appends ``<dest>/MOVED.jsonl`` (the volume path of
   every member is ``<volume_root>/<member path>``), then, on the Pod, writes
   ``state/moved/<run>-<label>.json`` and deletes the seal's files.

The moved marker first records the verified copy (``deleted`` false); it says
``deleted`` true only once every file is confirmed gone, and only then does the
driver count the block moved, so an interrupted deletion is retried. Nothing on
the volume is deleted before step 2 passes. Before the lease controller's
recovery reads the volume it writes ``records-quiesce.json``; the follower then
moves nothing more and acknowledges (``records-quiesced.json``). Each cycle
records what the volume still holds (``records-backlog.json``), so a lease
released with sealed blocks unmoved leaves them named for the next lease, whose
follower drains them first. A failed cycle leaves the
volume copy, writes ``<control>/ALERT-records.json`` and retries; the driver's
own latch stops new blocks while the copy-back moves nothing (``records.stall_seconds``)
or the window of unmoved blocks is full. An archive that would pass the
study-total budget on E: (``--budget-bytes``) is refused the same way.

CLI:
  follow --control LEASE_DIR --package PACKAGE_DIR --dest E:/g-i-records --budget-bytes N
         [--ssh-key KEY] [--interval 60] [--once]
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import time

import nine_deck_cloud_v1 as cloud

LEVEL = 10
POD_LOCK = "/root/g-records.lock"  # Pod-local: the lease controller's recovery takes it to stop deletion
POD_QUIESCE = "/root/g-records-quiesce"  # Pod-local, so a new Pod starts unquiesced
BUDGET_ALERT = 0.8
NO_POD_SECONDS = 2 * 3600
SCHEMA = "g-references-v1/records-moved"
SEAL_SCHEMA = "g-references-v1/block-records"

LIST = '''import os
seals, moved = ARGS["state"] + "/seals", ARGS["state"] + "/moved"
names = sorted(n for n in os.listdir(seals) if n.endswith(".json")) if os.path.isdir(seals) else []
def deleted(name):
    try:
        return json.load(open(moved + "/" + name)).get("deleted") is True
    except (OSError, ValueError):
        return False
print(json.dumps({"pending": [n for n in names if not deleted(n)]}))
'''

READ = '''path = ARGS["state"] + "/seals/" + ARGS["name"]
print(json.dumps({"seal": json.load(open(path)), "sha256": digest(path)}))
'''

TAR = '''import json, os, sys, tarfile
seal = json.load(open(sys.argv[1]))
with tarfile.open(fileobj=sys.stdout.buffer, mode="w|", format=tarfile.PAX_FORMAT) as tar:
    for item in seal["files"]:
        tar.add(os.path.join(seal["root"], item["path"]), arcname=item["path"], recursive=False)
'''

FINALIZE = '''import os
try:
    import fcntl
    lock = open(ARGS["lock"], "a")
    fcntl.flock(lock, fcntl.LOCK_EX)  # held through the deletion; recovery waits for it
except ImportError:
    lock = None
need(not os.path.exists(ARGS["quiesce"]), "the copy-back is quiesced for the lease's recovery")
path = ARGS["state"] + "/seals/" + ARGS["name"]
need(digest(path) == ARGS["seal_sha256"], "seal changed since it was copied")
seal = json.load(open(path))
os.makedirs(ARGS["state"] + "/moved", exist_ok=True)
marker = ARGS["state"] + "/moved/" + ARGS["name"]
def publish(value):
    with open(marker + ".partial", "w") as stream:
        json.dump(value, stream, indent=1, sort_keys=True)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(marker + ".partial", marker)
publish({**ARGS["marker"], "deleted": False})  # copied and verified; the volume copy not yet gone
parents = set()
for item in seal["files"]:
    target = os.path.join(seal["root"], item["path"])
    if os.path.exists(target):
        os.unlink(target)
    parent = os.path.dirname(item["path"])
    while parent.count("/") >= 2:  # keep the top folders (retained/G3, hot/G3, state/configs)
        parents.add(parent)
        parent = os.path.dirname(parent)
for parent in sorted(parents, key=lambda p: -p.count("/")):
    try:
        os.rmdir(os.path.join(seal["root"], parent))
    except OSError:
        pass
left = [item["path"] for item in seal["files"] if os.path.lexists(os.path.join(seal["root"], item["path"]))]
need(not left, "volume copy not gone: " + ", ".join(left[:5]))
publish({**ARGS["marker"], "deleted": True})  # only now does the driver count the block moved
print(json.dumps({"deleted": len(seal["files"])}))
'''


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def zstd_codec():
    """(writer, reader) of zstd files: Python 3.14's compression.zstd on Jack's PC, no package needed."""
    from compression import zstd
    return (lambda path: zstd.ZstdFile(path, "wb", level=LEVEL)), (lambda path: zstd.ZstdFile(path))


def verify(archive: Path, seal: dict, reader) -> None:
    """The archive holds exactly the seal's files, each with the seal's bytes and SHA256."""
    expected = {item["path"]: (item["bytes"], item["sha256"]) for item in seal["files"]}
    seen = {}
    with reader(archive) as stream, tarfile.open(fileobj=stream, mode="r|") as tar:
        for member in tar:
            cloud.require(member.isfile() and member.name not in seen, f"unexpected archive member {member.name}")
            data, digest, size = tar.extractfile(member), hashlib.sha256(), 0
            for chunk in iter(lambda: data.read(1 << 20), b""):
                digest.update(chunk)
                size += len(chunk)
            seen[member.name] = (size, digest.hexdigest())
    cloud.require(seen == expected, "archive members differ from the seal")


def moved_rows(dest: Path) -> list[dict]:
    """MOVED.jsonl's rows. A torn last line (an append cut short) is dropped, rewriting the ledger atomically
    with every complete row kept; a damaged earlier row is an error."""
    path = dest / "MOVED.jsonl"
    if not path.exists():
        return []
    lines = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
    rows = []
    for index, line in enumerate(lines):
        try:
            rows.append(json.loads(line))
        except ValueError:
            cloud.require(index == len(lines) - 1, f"MOVED.jsonl row {index + 1} is damaged")
            publish(path, "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows))
    return rows


def publish(path: Path, text: str) -> None:
    """Write a file atomically and durably (temporary file, fsync, replace)."""
    temporary = path.with_name(path.name + ".partial")
    with temporary.open("w", encoding="utf-8") as handle:
        handle.write(text)
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, path)


def archived_bytes(dest: Path) -> int:
    return sum(row["archive_bytes"] for row in moved_rows(dest))


def move(pod, state: str, name: str, dest: Path, budget: int, stream=subprocess.Popen, codec=None) -> dict:
    """Copy, verify and record one sealed block, then delete its volume copy."""
    writer, reader = codec or zstd_codec()
    read = pod.run(READ, {"state": state, "name": name}, timeout=120, read_only=True)
    seal, seal_sha256 = read["seal"], read["sha256"]
    cloud.require(seal.get("schema") == SEAL_SCHEMA and f"{seal['run']}-" in name, "not a block-records seal")
    folder = dest / seal["run"]
    folder.mkdir(parents=True, exist_ok=True)
    final = folder / name.replace(".json", ".tar.zst")
    partial = final.with_name(final.name + ".partial")
    sidecar = folder / name.replace(".json", ".seal.json")
    if not final.exists():
        with stream([*pod.ssh, "python3", "-B", "-", f"{state}/seals/{name}"], stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE, stderr=subprocess.PIPE) as child:
            child.stdin.write(TAR.encode())
            child.stdin.close()
            with writer(partial) as out:
                for chunk in iter(lambda: child.stdout.read(1 << 20), b""):
                    out.write(chunk)
            error = child.stderr.read().decode(errors="replace")
        cloud.require(child.returncode == 0, "record stream failed: " + error[-400:])
        verify(partial, seal, reader)
        size, used = partial.stat().st_size, archived_bytes(dest)
        if used + size > budget:
            partial.unlink()
            raise RuntimeError(f"the study's record budget on E: ({budget} B) would be passed")
        if used + size > BUDGET_ALERT * budget:  # warn well before the budget stalls the study
            (dest / "ALERT-budget.json").write_text(json.dumps({"used_bytes": used + size, "budget_bytes": budget}),
                                                    encoding="utf-8")
        with partial.open("rb+") as handle:
            os.fsync(handle.fileno())
        os.replace(partial, final)
    verify(final, seal, reader)  # again from its final place, before anything on the volume goes
    try:
        current = json.loads(sidecar.read_text(encoding="utf-8"))
    except (FileNotFoundError, ValueError):  # missing, or torn by a crash: written again from the verified seal
        current = None
    if current != seal:  # the archive's own seal, durable before anything on the volume goes
        publish(sidecar, json.dumps(seal, indent=1, sort_keys=True))
    archive_sha256 = sha256_file(final)
    row = {"schema": SCHEMA, "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "run": seal["run"],
           "block": seal["block"], "seal": name, "seal_sha256": seal_sha256, "volume_root": seal["root"],
           "archive": str(final), "archive_sha256": archive_sha256, "archive_bytes": final.stat().st_size,
           "files": len(seal["files"]), "raw_bytes": seal["bytes"], "level": LEVEL}
    moved = dest / "MOVED.jsonl"
    rows = moved_rows(dest)
    if not any((other["run"], other["block"]) == (seal["run"], seal["block"]) for other in rows):
        if moved.exists() and moved.stat().st_size and not moved.read_bytes().endswith(b"\n"):
            publish(moved, "".join(json.dumps(other, sort_keys=True) + "\n" for other in rows))  # every kept row
        with moved.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(row, sort_keys=True) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
    pod.run(FINALIZE, {"state": state, "name": name, "seal_sha256": seal_sha256, "lock": POD_LOCK,
                       "quiesce": POD_QUIESCE,
                       "marker": {key: row[key] for key in ("archive", "archive_sha256", "archive_bytes", "at")}},
            timeout=600)
    return row


def quiesced(control: Path) -> bool:
    """The lease controller asked the copy-back to stop before its recovery reads the volume: acknowledge."""
    request = control / "records-quiesce.json"
    if not request.exists():
        return False
    (control / "records-quiesced.json").write_text(json.dumps({"epoch": time.time()}), encoding="utf-8")
    return True


def backlog(pod, state: str, control: Path) -> list[str]:
    """What the volume still holds, recorded in the lease folder (the last record names a released lease's
    unmoved blocks for the next lease)."""
    pending = pod.run(LIST, {"state": state}, timeout=120, read_only=True)["pending"]
    publish(control / "records-backlog.json", json.dumps(
        {"at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "state": state, "pending": pending}, indent=1))
    return pending


def cycle(pod, state: str, dest: Path, budget: int, control: Path, **options) -> int:
    """Move every pending sealed block; on a failure, alert and stop this cycle (the next one retries)."""
    pending = backlog(pod, state, control)
    moved = 0
    for name in pending:
        if quiesced(control):
            break
        try:
            move(pod, state, name, dest, budget, **options)
            moved += 1
        except Exception as error:  # the volume copy stays; the driver's stall latch stops new blocks
            alert = {"at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "seal": name,
                     "kind": "transport" if isinstance(error, cloud.PodUnavailable) else "copy",
                     "error": f"{type(error).__name__}: {error}"[-2000:]}
            (control / "ALERT-records.json").write_text(json.dumps(alert, indent=1), encoding="utf-8")
            break
    backlog(pod, state, control)
    return moved


def follow(control: Path, package: Path, dest: Path, budget: int, key: Path, interval: float, once: bool) -> int:
    state = json.loads((package / "campaign.json").read_text(encoding="utf-8"))["roots"]["state"]
    publish(control / "records-follow-ready.json", json.dumps({"epoch": time.time(), "pid": os.getpid(),
                                                              "dest": str(dest), "budget_bytes": budget}))
    started = time.monotonic()
    while True:
        if not (control / "endpoint.json").exists() and time.monotonic() - started > NO_POD_SECONDS:
            return 1  # the lease never reached a Pod: nothing to follow
        if (control / "endpoint.json").exists():
            pod = cloud.Pod(json.loads((control / "endpoint.json").read_text(encoding="utf-8")), key,
                            control / "known_hosts", subprocess.run, time.sleep)
            try:
                if quiesced(control):
                    backlog(pod, state, control)  # refreshed after quiescence, for the release record
                else:
                    cycle(pod, state, dest, budget, control)
            except cloud.PodUnavailable:
                pass  # the Pod is starting or gone; nothing was deleted
        if once or (control / "release-authorized.json").exists():
            return 0
        time.sleep(interval)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("action", choices=("follow",))
    parser.add_argument("--control", type=Path, required=True)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--dest", type=Path, required=True)
    parser.add_argument("--budget-bytes", type=int, required=True)
    parser.add_argument("--ssh-key", type=Path, default=Path.home() / ".ssh" / "id_ed25519")
    parser.add_argument("--interval", type=float, default=60.0)
    parser.add_argument("--once", action="store_true")
    args = parser.parse_args(argv)
    return follow(args.control, args.package, args.dest, args.budget_bytes, args.ssh_key, args.interval, args.once)


if __name__ == "__main__":
    sys.exit(main())
