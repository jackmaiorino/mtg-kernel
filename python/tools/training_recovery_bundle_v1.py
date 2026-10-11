"""Durable, byte-exact recovery of an explicit file inventory in one STORE ZIP.

The caller selects the entire payload and checks physical-device independence.
This module never omits duplicate content, retries, overwrites, or deletes failed
attempts. Success requires independent destination inventory and SHA readback.
"""
from __future__ import annotations

import ctypes
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import time
from typing import Iterable
import zipfile


SCHEMA = "training-recovery-bundle/v1"
MANIFEST = "recovery-manifest.json"
CHUNK = 1024 * 1024


def _require(condition, message):
    if not condition:
        raise ValueError(message)


def _safe_path(value: Path | str) -> Path:
    path = Path(value)
    _require(path.is_absolute() and ".." not in path.parts,
             "absolute path without traversal required")
    for part in path.parts[1:]:
        _relative(part)  # Reject Windows alternate streams and ambiguous names.
    for entry in (*reversed(path.parents), path):
        try:
            metadata = entry.lstat()
        except FileNotFoundError:
            continue
        _require(not stat.S_ISLNK(metadata.st_mode)
                 and not (getattr(metadata, "st_file_attributes", 0) & 0x400),
                 "link/reparse path refused: " + str(entry))
    return path


def _regular(path: Path):
    _safe_path(path)
    metadata = path.lstat()
    _require(stat.S_ISREG(metadata.st_mode) and metadata.st_nlink == 1,
             "regular unlinked file required: " + str(path))
    return metadata


def _fingerprint(metadata):
    return (metadata.st_dev, metadata.st_ino, metadata.st_size,
            metadata.st_mtime_ns, metadata.st_ctime_ns, metadata.st_nlink)


def _relative(value: str) -> str:
    _require(isinstance(value, str) and value, "relative path must be text")
    path = PurePosixPath(value)
    _require(path.parts and not path.is_absolute() and path.as_posix() == value,
             "noncanonical relative path")
    for part in path.parts:
        _require(part not in (".", "..") and not part.endswith((" ", "."))
                 and not re.search(r'[\\<>:"|?*\x00-\x1f\x7f]', part)
                 and not re.fullmatch(r"(?i)(CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?", part),
                 "unsafe relative path: " + value)
    return value


def _inventory(items: Iterable[dict]) -> list[dict]:
    result, names = [], set()
    for item in items:
        _require(isinstance(item, dict) and set(item) == {"relative_path", "bytes", "sha256"},
                 "invalid inventory record")
        name = _relative(item["relative_path"])
        _require(type(item["bytes"]) is int and item["bytes"] >= 0
                 and isinstance(item["sha256"], str)
                 and re.fullmatch("[a-f0-9]{64}", item["sha256"]),
                 "invalid inventory size/SHA")
        _require(name.casefold() not in names, "duplicate recovery path")
        names.add(name.casefold())
        result.append(dict(item))
    _require(result, "empty recovery inventory")
    for name in names:
        _require(not any(parent.as_posix().casefold() in names
                         for parent in PurePosixPath(name).parents if parent.as_posix() != "."),
                 "file/directory inventory collision")
    return sorted(result, key=lambda item: item["relative_path"])


def _sha_file(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def _sync_directory(path: Path):
    if os.name != "nt":
        descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
        try:
            os.fsync(descriptor)
        finally:
            os.close(descriptor)


def _mkdir_parents(path: Path):
    _safe_path(path)
    missing = []
    cursor = path
    while not cursor.exists():
        missing.append(cursor)
        cursor = cursor.parent
    for entry in reversed(missing):
        _safe_path(entry)
        entry.mkdir()
        _sync_directory(entry.parent)
    _safe_path(path)


def _publish(temporary: Path, destination: Path):
    """Publish without replacement; Windows requests write-through rename."""
    _safe_path(temporary)
    _safe_path(destination)
    if os.name == "nt":
        move = ctypes.WinDLL("kernel32", use_last_error=True).MoveFileExW
        move.argtypes = [ctypes.c_wchar_p, ctypes.c_wchar_p, ctypes.c_ulong]
        move.restype = ctypes.c_int
        if not move(str(temporary), str(destination), 0x8):  # MOVEFILE_WRITE_THROUGH
            raise ctypes.WinError(ctypes.get_last_error())
    else:
        os.link(temporary, destination, follow_symlinks=False)
        temporary.unlink()
        _sync_directory(destination.parent)


class _SequentialWriter(io.RawIOBase):
    """Force sequential ZIP descriptors so the published archive has a digest."""
    def __init__(self, stream):
        super().__init__()
        self.stream = stream
        self.digest = hashlib.sha256()
        self.position = 0

    def writable(self):
        return True

    def tell(self):
        return self.position

    def write(self, data):
        written = self.stream.write(data)
        _require(written == len(data), "short bundle write")
        self.digest.update(data)
        self.position += written
        return written

    def flush(self):
        self.stream.flush()


def _zip_info(name: str) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_STORED
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | 0o600) << 16
    return info


def create_bundle(source_root: Path | str, sources: Iterable[Path | str],
                  destination: Path | str, *, expected_inventory=None,
                  reserve_bytes: int = 0) -> dict:
    """Stream each selected source once, publish, and independently verify.

    ``sources`` must contain every payload file, without duplicate paths. Source
    bytes are hashed while streaming and file identity/metadata is checked both
    around each read and after the entire inventory. Optional expected_inventory
    adds caller-supplied SHA pins. Failures retain the .partial ZIP or published
    archive and raise; no success receipt is returned.
    """
    began = time.monotonic()
    root = _safe_path(source_root)
    _require(root.is_dir(), "source root must be a directory")
    destination = _safe_path(destination)
    temporary = destination.with_name(destination.name + ".partial")
    _safe_path(temporary)
    _require(not destination.exists() and not temporary.exists(), "preserve prior bundle attempt")
    _require(type(reserve_bytes) is int and reserve_bytes >= 0, "invalid storage reserve")
    jobs, names = [], set()
    for value in sources:
        source = _safe_path(value)
        _require(source.is_relative_to(root), "source outside recovery root")
        relative = _relative(source.relative_to(root).as_posix())
        _require(relative.casefold() not in names, "duplicate recovery source")
        names.add(relative.casefold())
        jobs.append((relative, source, _fingerprint(_regular(source))))
    _require(jobs, "empty recovery inventory")
    jobs.sort()
    expected = _inventory(expected_inventory) if expected_inventory is not None else None
    if expected is not None:
        _require([item["relative_path"] for item in expected] == [job[0] for job in jobs],
                 "expected inventory differs from selected sources")
    payload_bytes = sum(identity[2] for _, _, identity in jobs)
    # Bound STORE headers, descriptors, central directory and JSON inventory.
    overhead = 4096 + sum(1024 + 6 * len(relative.encode("utf-8")) for relative, _, _ in jobs)
    _mkdir_parents(destination.parent)
    _require(shutil.disk_usage(destination.parent).free >= reserve_bytes + payload_bytes + overhead,
             "bundle would violate destination storage reserve")
    timing = {"inventory_seconds": time.monotonic() - began}
    phase = time.monotonic()
    inventory = []
    with temporary.open("xb") as output:
        writer = _SequentialWriter(output)
        with zipfile.ZipFile(writer, "w", compression=zipfile.ZIP_STORED, allowZip64=True) as archive:
            for index, (relative, source, identity) in enumerate(jobs):
                _require(_fingerprint(_regular(source)) == identity, "source mutated before read")
                digest, size = hashlib.sha256(), 0
                descriptor = os.open(source, os.O_RDONLY | getattr(os, "O_BINARY", 0)
                                     | getattr(os, "O_NOFOLLOW", 0))
                with os.fdopen(descriptor, "rb") as original:
                    _require(_fingerprint(os.fstat(original.fileno())) == identity,
                             "source replaced during open")
                    with archive.open(_zip_info("payload/" + relative), "w", force_zip64=True) as member:
                        while block := original.read(CHUNK):
                            member.write(block)
                            digest.update(block)
                            size += len(block)
                    _require(_fingerprint(os.fstat(original.fileno())) == identity
                             and _fingerprint(_regular(source)) == identity and size == identity[2],
                             "source mutated during read")
                record = {"relative_path": relative, "bytes": size, "sha256": digest.hexdigest()}
                if expected is not None:
                    _require(record == expected[index], "source bytes differ from expected inventory")
                inventory.append(record)
            manifest = {"schema": SCHEMA, "files": inventory}
            archive.writestr(_zip_info(MANIFEST), json.dumps(manifest, sort_keys=True,
                             separators=(",", ":"), ensure_ascii=True).encode("utf-8"))
        for _, source, identity in jobs:
            _require(_fingerprint(_regular(source)) == identity, "source mutated before publication")
        timing["stream_seconds"] = time.monotonic() - phase
        phase = time.monotonic()
        output.flush()
        os.fsync(output.fileno())
        timing["fsync_seconds"] = time.monotonic() - phase
        archive_sha, archive_bytes = writer.digest.hexdigest(), writer.position
        writer.close()
    phase = time.monotonic()
    _publish(temporary, destination)
    timing["publish_seconds"] = time.monotonic() - phase
    phase = time.monotonic()
    verification = verify_bundle(destination, inventory, expected_bundle_sha256=archive_sha)
    timing["verify_seconds"] = time.monotonic() - phase
    return {"schema": SCHEMA, "complete": True, "source_root": str(root),
            "bundle": {"path": str(destination), "bytes": archive_bytes, "sha256": archive_sha},
            "inventory": inventory, "payload_bytes": payload_bytes, "file_count": len(inventory),
            "verification": verification, "timing": timing, "seconds": time.monotonic() - began}


def _read_manifest(archive: zipfile.ZipFile, expected: list[dict]):
    infos = archive.infolist()
    desired = [MANIFEST] + ["payload/" + item["relative_path"] for item in expected]
    _require(len(infos) == len(desired) and {info.filename for info in infos} == set(desired),
             "recovery ZIP inventory differs")
    for info in infos:
        _require(info.compress_type == zipfile.ZIP_STORED and not (info.flag_bits & 1)
                 and not info.is_dir() and stat.S_ISREG(info.external_attr >> 16),
                 "non-STORE/encrypted/link recovery member refused")
    # The manifest is data, bounded by the externally supplied inventory.
    manifest_info = archive.getinfo(MANIFEST)
    bound = 4096 + sum(1024 + 6 * len(item["relative_path"].encode("utf-8")) for item in expected)
    _require(manifest_info.file_size <= bound, "oversized recovery manifest")
    manifest = json.loads(archive.read(manifest_info))
    _require(isinstance(manifest, dict) and set(manifest) == {"schema", "files"}
             and manifest["schema"] == SCHEMA and manifest["files"] == expected,
             "recovery manifest differs from expected inventory")


def verify_bundle(bundle: Path | str, expected_inventory: Iterable[dict], *,
                  expected_bundle_sha256: str | None = None) -> dict:
    """Reopen destination and hash its entire container and every STORE member.

    Require external expected_inventory, rather than trusting a self-declared
    manifest. Verification never reads the original source payload.
    """
    began = time.monotonic()
    expected = _inventory(expected_inventory)
    path = _safe_path(bundle)
    identity = _fingerprint(_regular(path))
    digest = _sha_file(path)
    if expected_bundle_sha256 is not None:
        _require(digest == expected_bundle_sha256, "recovery bundle SHA differs")
    with zipfile.ZipFile(path, "r") as archive:
        _read_manifest(archive, expected)
        for item in expected:
            member = archive.getinfo("payload/" + item["relative_path"])
            _require(member.file_size == item["bytes"], "recovery member size differs")
            sha, size = hashlib.sha256(), 0
            with archive.open(member) as stream:
                while block := stream.read(CHUNK):
                    sha.update(block)
                    size += len(block)
            _require(size == item["bytes"] and sha.hexdigest() == item["sha256"],
                     "recovery member SHA/size differs: " + item["relative_path"])
    _require(_fingerprint(_regular(path)) == identity, "recovery bundle mutated during verification")
    return {"complete": True, "independent_destination_readback": True,
            "bundle_sha256": digest, "file_count": len(expected),
            "payload_bytes": sum(item["bytes"] for item in expected),
            "seconds": time.monotonic() - began}


def extract_bundle(bundle: Path | str, destination: Path | str,
                   expected_inventory: Iterable[dict], *,
                   expected_bundle_sha256: str | None = None) -> dict:
    """Restore to a fresh root, then independently verify all restored bytes.

    A failed extraction is retained and cannot be retried into the same root.
    This proof helper intentionally fsyncs extracted files; normal bundle copy
    timing includes ZIP creation/readback, not a redundant expanded file copy.
    """
    began = time.monotonic()
    expected = _inventory(expected_inventory)
    path = _safe_path(bundle)
    root = _safe_path(destination)
    _require(not root.exists(), "fresh recovery extraction root required")
    verification = verify_bundle(path, expected, expected_bundle_sha256=expected_bundle_sha256)
    identity = _fingerprint(_regular(path))
    _mkdir_parents(root.parent)
    root.mkdir()
    _sync_directory(root.parent)
    with zipfile.ZipFile(path, "r") as archive:
        _read_manifest(archive, expected)
        for item in expected:
            target = _safe_path(root.joinpath(*PurePosixPath(item["relative_path"]).parts))
            _mkdir_parents(target.parent)
            with archive.open("payload/" + item["relative_path"]) as original, target.open("xb") as output:
                shutil.copyfileobj(original, output, length=CHUNK)
                output.flush()
                os.fsync(output.fileno())
            _sync_directory(target.parent)
    _require(_fingerprint(_regular(path)) == identity, "bundle mutated during extraction")
    actual = []
    for folder, directories, filenames in os.walk(root, followlinks=False):
        for name in directories:
            _safe_path(Path(folder) / name)
        for name in filenames:
            target = Path(folder) / name
            identity = _fingerprint(_regular(target))
            sha = _sha_file(target)
            _require(_fingerprint(_regular(target)) == identity, "extracted file mutated during readback")
            actual.append({"relative_path": target.relative_to(root).as_posix(),
                           "bytes": identity[2], "sha256": sha})
    _require(_inventory(actual) == expected, "extracted recovery inventory/SHA differs")
    return {"schema": "training-recovery-extraction/v1", "complete": True,
            "destination_root": str(root), "inventory": expected,
            "bundle_verification": verification, "file_count": len(expected),
            "payload_bytes": sum(item["bytes"] for item in expected),
            "independent_extracted_readback": True, "seconds": time.monotonic() - began}
