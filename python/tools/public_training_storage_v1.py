"""Storage-bound training allocation with verified compressed canonical archives."""
from concurrent.futures import ThreadPoolExecutor
import hashlib
import os
from pathlib import Path
import time
import zipfile

from public_training_dispatch_v2 import read, write, pin, checked, dispatch_qualification, dispatch_stack_qualification, _dispatch_group
from compute_throughput_v2 import require_allocation
from public_evaluation_dispatch_v1 import inventory

ARCHIVE = "two-deflate1-shards-full-readback/v1"


def storage(snapshot, drive):
    part = next(p for p in snapshot["partitions"] if p["DriveLetter"] == drive)
    disk = next(d for d in snapshot["disks"] if d["Number"] == part["DiskNumber"])
    volume = next(v for v in snapshot["volumes"] if v["DriveLetter"] == drive)
    return dict(drive=drive, disk_serial=disk["SerialNumber"], disk_name=disk["FriendlyName"],
                free_bytes=volume["SizeRemaining"])


def archive_native(native, root, validated_files=None):
    before = time.monotonic()
    files = sorted(p for p in native.rglob("*") if p.is_file())
    def shard(index):
        path = root/f"native-{index}.zip"
        hashes = {}
        sizes = {}
        write_started = time.monotonic()
        with zipfile.ZipFile(path, "x", compression=zipfile.ZIP_DEFLATED, compresslevel=1, allowZip64=True) as archive:
            for source in files[index::2]:
                name = source.relative_to(native).as_posix()
                expected = (validated_files or {}).get(str(source.resolve()))
                if expected is None:
                    expected = pin(source)["sha256"]
                # Hash exactly the bytes supplied to ZIP, in one source pass.
                # Keep ZipFile.write's metadata and compression settings.
                info = zipfile.ZipInfo.from_file(source, name)
                info.compress_type = archive.compression
                info._compresslevel = archive.compresslevel
                digest = hashlib.sha256()
                count = 0
                with source.open("rb") as stream, archive.open(info, "w", force_zip64=info.file_size * 1.05 > zipfile.ZIP64_LIMIT) as destination:
                    while chunk := stream.read(1024 * 1024):
                        digest.update(chunk)
                        destination.write(chunk)
                        count += len(chunk)
                hashes[name] = digest.hexdigest()
                assert hashes[name] == expected, "source changed before or during archive: " + str(source)
                sizes[name] = count
        with path.open("r+b") as stream: os.fsync(stream.fileno())
        write_seconds = time.monotonic() - write_started
        readback_started = time.monotonic()
        with zipfile.ZipFile(path) as archive:
            assert set(archive.namelist()) == set(hashes)
            for name, digest in hashes.items():
                with archive.open(name) as stream:
                    assert hashlib.file_digest(stream, "sha256").hexdigest() == digest
        readback_seconds = time.monotonic() - readback_started
        pin_started = time.monotonic()
        archive_pin = pin(path)
        return dict(archive=archive_pin, files=hashes, source_bytes=sum(sizes.values()),
                    timing=dict(write_fsync_seconds=write_seconds, full_readback_seconds=readback_seconds,
                                archive_pin_seconds=time.monotonic()-pin_started))
    with ThreadPoolExecutor(max_workers=2) as pool: shards = list(pool.map(shard, [0, 1]))
    result = dict(scheme=ARCHIVE, native_root=str(native), seconds=time.monotonic()-before,
        source_files=len(files), source_bytes=sum(s["source_bytes"] for s in shards),
        compressed_bytes=sum(Path(s["archive"]["path"]).stat().st_size for s in shards),
        shards=shards, mismatches=0, raw_native_files_retained=True)
    write(root/"archive.json", result)
    return result


def dispatch(root, binary, configs, placements, store, updates, wall_seconds=2400, mode="parallel", stack=False):
    root.mkdir()
    snapshot = inventory("desktop")
    assert not snapshot["active"]
    current = storage(snapshot, store["drive"])
    assert current["disk_serial"] == store["disk_serial"] and current["disk_name"] == store["disk_name"]
    assert current["free_bytes"] > 60*1024**3, "preserve ample native-store headroom"
    write(root/"storage-before.json", snapshot)
    retries = []
    for attempt in range(3):
        native = Path(f"{store['drive']}:/mtg-training-working/{root.name}-native-{attempt}")
        native.parent.mkdir(exist_ok=True)
        write(root/f"dispatch-{attempt}.json", dict(binary=binary, configs=configs, placements=placements,
            native_root=str(native), local_storage=store, archive_scheme=ARCHIVE,
            updates=updates, wall_seconds=wall_seconds, mode=mode))
        try:
            if updates is not None:
                if stack:
                    assert mode == "device_queues"
                    group_pin = dispatch_stack_qualification(native,binary,configs,placements,updates=updates)
                else:
                    group_pin = dispatch_qualification(native,binary,configs,placements,mode,updates=updates)
            else:
                group_pin = _dispatch_group(native,binary,configs,placements,mode,None,wall_seconds)
            break
        except ValueError as error:
            # The dispatcher checks all hosts before creating any host job tree.
            # Retry only this precise preflight failure, never a started job.
            untouched = not any((native/host).exists() for host in ["desktop", "computehost"])
            if str(error) != "selected GPU is unavailable; preserve its current work" or not untouched:
                raise
            retries.append(dict(attempt=attempt, native_root=str(native), error=str(error), no_jobs_staged=True))
            write(root/f"preflight-retry-{attempt}.json", retries[-1])
            if attempt == 2:
                raise
            time.sleep(2)
    archived = archive_native(native,root)
    group = read(checked(group_pin))
    group.update(native_group=group_pin, archive=pin(root/"archive.json"), local_storage=store,
        storage_before=pin(root/"storage-before.json"), archive_scheme=ARCHIVE,
        recovery_seconds=group["recovery_seconds"]+archived["seconds"])
    write(root/"group-benchmark.json",group)
    return pin(root/"group-benchmark.json")


def require_storage_choice(path,binary,configs,stack=False):
    choice = read(path)
    jobs = {arm:dict(config_sha256=item["sha256"],updates=len(read(checked(item))["updates"])) for arm,item in configs.items()}
    # Existing guard verifies same full configs, native GPU identities, complete
    # learning outputs, actual concurrent timing, and the archive-inclusive minimum.
    validator = require_allocation
    if stack:
        from compute_throughput_v3 import require_allocation as validator
    selected = validator(path,binary["sha256"],jobs)
    assert choice["archive_scheme"] == ARCHIVE
    for item in choice["storage_dependencies"]: checked(item)
    measured = {}
    selected_store = None
    for candidate in choice["candidates"]:
        group = read(checked(candidate["benchmark"]))
        archived = read(checked(group["archive"]))
        assert group["archive_scheme"] == archived["scheme"] == ARCHIVE and archived["mismatches"] == 0
        native = read(checked(group["native_group"]))
        assert group["recovery_seconds"] == native["recovery_seconds"]+archived["seconds"]
        store = group["local_storage"]
        snapshot = read(checked(group["storage_before"]))
        actual = storage(snapshot,store["drive"])
        assert actual["disk_serial"] == store["disk_serial"] and actual["disk_name"] == store["disk_name"]
        assert Path(archived["native_root"]).drive.upper() == store["drive"]+":"
        for arm,item in group["jobs"].items():
            report = read(checked(item))
            if report["placement"]["host"] == "desktop":
                assert Path(report["source_output_directory"]).drive.upper() == store["drive"]+":"
                measured.setdefault((store["drive"],store["disk_serial"]),set()).add(report["placement"]["workers"])
        for shard in archived["shards"]: checked(shard["archive"])
        if candidate["id"] == selected["id"]: selected_store = store
    assert set(measured) == {tuple(row) for row in choice["eligible_local_storage"]}
    assert all(1 in counts and any(n>1 for n in counts) for counts in measured.values())
    assert selected_store is not None
    return dict(**selected, local_storage=selected_store, archive_scheme=ARCHIVE)


def dispatch_qualified(root,binary,configs,choice_path,wall_seconds):
    selected = require_storage_choice(choice_path,binary,configs)
    return dispatch(root,binary,configs,selected["placements"],selected["local_storage"],None,wall_seconds,selected["mode"])


def dispatch_stack_qualified(root,binary,configs,choice_path,wall_seconds):
    selected = require_storage_choice(choice_path,binary,configs,stack=True)
    if selected["mode"] != "device_queues":
        raise ValueError("stack allocation must use per-device queues")
    return dispatch(root,binary,configs,selected["placements"],selected["local_storage"],None,wall_seconds,selected["mode"],stack=True)
