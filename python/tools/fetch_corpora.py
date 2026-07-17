#!/usr/bin/env python3
"""Fetch, safely extract, and verify the formal Burn/Rally replay corpora."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import stat
import tempfile
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any, NoReturn


REPO_ROOT = Path(__file__).resolve().parents[2]
ARCHIVE_CATALOG_PATH = REPO_ROOT / "corpus_archives_v1.json"
CONTENT_LOCK_PATH = REPO_ROOT / "corpus_content_locks_v1.json"
_CHUNK_SIZE = 1024 * 1024
_LOWER_SHA256 = re.compile(r"[0-9a-f]{64}")
_REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)
_WINDOWS_RESERVED_COMPONENT = re.compile(
    r"(?i)(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?"
)


class CorpusFetchError(RuntimeError):
    """Raised when corpus retrieval or verification fails closed."""


def _reject_constant(value: str) -> NoReturn:
    raise CorpusFetchError(f"non-finite JSON number is forbidden: {value}")


def _strict_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for key, value in pairs:
        if key in out:
            raise CorpusFetchError(f"duplicate JSON key: {key!r}")
        out[key] = value
    return out


def _load_json(path: Path) -> Any:
    try:
        return json.loads(
            path.read_text(encoding="utf-8"),
            object_pairs_hook=_strict_object,
            parse_constant=_reject_constant,
        )
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise CorpusFetchError(f"cannot read strict JSON {path}: {exc}") from exc


def _sha256_file(path: Path) -> tuple[int, str]:
    digest = hashlib.sha256()
    size = 0
    with path.open("rb") as handle:
        while chunk := handle.read(_CHUNK_SIZE):
            size += len(chunk)
            digest.update(chunk)
    return size, digest.hexdigest()


def _is_safe_component(value: Any) -> bool:
    return (
        isinstance(value, str)
        and bool(value)
        and value not in {".", ".."}
        and PurePosixPath(value).name == value
        and value[-1] not in {".", " "}
        and not any(
            ord(character) < 32 or character in '<>:"/\\|?*'
            for character in value
        )
        and _WINDOWS_RESERVED_COMPONENT.fullmatch(value) is None
    )


def _catalogs() -> tuple[dict[str, Any], dict[str, dict[str, Any]]]:
    catalog = _load_json(ARCHIVE_CATALOG_PATH)
    locks = _load_json(CONTENT_LOCK_PATH)
    if (
        not isinstance(catalog, dict)
        or set(catalog) != {"schema_version", "repository", "release_tag", "archives"}
        or catalog.get("schema_version") != 1
    ):
        raise CorpusFetchError("unsupported corpus archive catalog")
    repository = catalog["repository"]
    repository_parts = repository.split("/") if isinstance(repository, str) else []
    if len(repository_parts) != 2 or not all(
        _is_safe_component(component) for component in repository_parts
    ):
        raise CorpusFetchError("archive catalog repository must be owner/name")
    if not _is_safe_component(catalog["release_tag"]):
        raise CorpusFetchError("archive catalog release tag must be one safe component")
    if (
        not isinstance(locks, dict)
        or set(locks) != {"schema_version", "corpora"}
        or locks.get("schema_version") != 1
    ):
        raise CorpusFetchError("unsupported corpus content-lock catalog")
    corpora = locks["corpora"]
    if not isinstance(corpora, list) or not corpora:
        raise CorpusFetchError("content-lock catalog has no corpora list")
    lock_by_directory: dict[str, dict[str, Any]] = {}
    folded_directories: set[str] = set()
    for item in corpora:
        if not isinstance(item, dict) or not _is_safe_component(item.get("directory")):
            raise CorpusFetchError("malformed content-lock corpus record")
        directory = item["directory"]
        folded = directory.casefold()
        if directory in lock_by_directory or folded in folded_directories:
            raise CorpusFetchError(f"duplicate content-lock corpus directory: {directory}")
        lock_by_directory[directory] = item
        folded_directories.add(folded)
    return catalog, lock_by_directory


def _trace_map(lock: dict[str, Any]) -> dict[str, dict[str, Any]]:
    traces = lock.get("traces")
    if not isinstance(traces, list) or not traces:
        raise CorpusFetchError("content lock has no trace list")
    out: dict[str, dict[str, Any]] = {}
    for record in traces:
        if not isinstance(record, dict) or not isinstance(record.get("path"), str):
            raise CorpusFetchError("malformed trace lock record")
        path = record["path"]
        if path in out:
            raise CorpusFetchError(f"duplicate trace lock path: {path}")
        out[path] = record
    return out


def _is_link_or_reparse(path: Path) -> bool:
    try:
        metadata = os.lstat(path)
    except OSError as exc:
        raise CorpusFetchError(f"cannot inspect corpus path {path}: {exc}") from exc
    return stat.S_ISLNK(metadata.st_mode) or bool(
        getattr(metadata, "st_file_attributes", 0) & _REPARSE_POINT
    )


def _regular_corpus_files(root: Path) -> list[Path]:
    files: list[Path] = []
    pending = [root]
    while pending:
        directory = pending.pop()
        try:
            entries = list(os.scandir(directory))
        except OSError as exc:
            raise CorpusFetchError(f"cannot enumerate corpus directory {directory}: {exc}") from exc
        for entry in entries:
            path = Path(entry.path)
            if _is_link_or_reparse(path):
                raise CorpusFetchError(f"links and reparse points are forbidden in a corpus: {path}")
            if entry.is_dir(follow_symlinks=False):
                pending.append(path)
            elif entry.is_file(follow_symlinks=False):
                files.append(path)
            else:
                raise CorpusFetchError(f"non-regular corpus entry is forbidden: {path}")
    return files


def verify_corpus(root: Path, lock: dict[str, Any]) -> None:
    """Verify one extracted corpus against the tracked raw-byte content lock."""
    if not root.is_dir() or _is_link_or_reparse(root):
        raise CorpusFetchError(f"corpus root is missing, not a directory, or a symlink: {root}")
    manifest_path = root / "manifest.json"
    manifest = _load_json(manifest_path)
    if not isinstance(manifest, dict):
        raise CorpusFetchError("corpus manifest must be a JSON object")
    if manifest.get("corpus") != lock.get("manifest_corpus"):
        raise CorpusFetchError("corpus manifest identity does not match the content lock")
    if manifest.get("status") != lock.get("required_status"):
        raise CorpusFetchError("corpus manifest status does not match the content lock")

    expected = _trace_map(lock)
    actual: dict[str, Path] = {}
    for path in _regular_corpus_files(root):
        if path.name.startswith("game_") and path.suffix == ".txt":
            relative = path.relative_to(root).as_posix()
            actual[relative] = path
    if set(actual) != set(expected):
        raise CorpusFetchError("corpus trace path set does not match the content lock")

    aggregate = bytearray()
    for relative in sorted(expected, key=lambda value: value.encode("utf-8")):
        size, digest = _sha256_file(actual[relative])
        record = expected[relative]
        if size != record.get("size") or digest != record.get("sha256"):
            raise CorpusFetchError(f"trace content lock mismatch: {relative}")
        aggregate.extend(
            b"trace\0"
            + relative.encode("utf-8")
            + b"\0"
            + str(size).encode("ascii")
            + b"\0"
            + digest.encode("ascii")
            + b"\n"
        )

    manifest_size, manifest_digest = _sha256_file(manifest_path)
    manifest_lock = lock.get("manifest")
    if not isinstance(manifest_lock, dict):
        raise CorpusFetchError("malformed manifest content lock")
    if manifest_size != manifest_lock.get("size") or manifest_digest != manifest_lock.get("sha256"):
        raise CorpusFetchError("manifest content lock mismatch")
    aggregate.extend(
        b"manifest\0manifest.json\0"
        + str(manifest_size).encode("ascii")
        + b"\0"
        + manifest_digest.encode("ascii")
        + b"\n"
    )
    if hashlib.sha256(aggregate).hexdigest() != lock.get("aggregate_sha256"):
        raise CorpusFetchError("aggregate corpus content lock mismatch")


def _expected_archive_entries(directory: str, lock: dict[str, Any]) -> set[str]:
    return {
        f"{directory}/manifest.json",
        *(f"{directory}/{relative}" for relative in _trace_map(lock)),
    }


def _validate_archive_spec(
    archive_spec: dict[str, Any], lock: dict[str, Any]
) -> tuple[str, str, int, str]:
    expected_keys = {
        "directory",
        "asset",
        "size",
        "sha256",
        "content_lock_aggregate_sha256",
    }
    if set(archive_spec) != expected_keys:
        raise CorpusFetchError("archive catalog record has missing or unknown fields")
    directory = archive_spec["directory"]
    asset = archive_spec["asset"]
    size = archive_spec["size"]
    digest = archive_spec["sha256"]
    if (
        not _is_safe_component(directory)
    ):
        raise CorpusFetchError("archive directory must be one safe path component")
    if (
        not _is_safe_component(asset)
        or not asset.endswith(".zip")
    ):
        raise CorpusFetchError("archive asset must be one safe .zip filename")
    if type(size) is not int or size <= 0:
        raise CorpusFetchError("archive size must be a positive integer")
    if not isinstance(digest, str) or _LOWER_SHA256.fullmatch(digest) is None:
        raise CorpusFetchError("archive digest must be lowercase SHA-256")
    if lock.get("directory") != directory:
        raise CorpusFetchError("archive/content-lock directory mismatch")
    aggregate = lock.get("aggregate_sha256")
    if (
        not isinstance(aggregate, str)
        or _LOWER_SHA256.fullmatch(aggregate) is None
        or archive_spec["content_lock_aggregate_sha256"] != aggregate
    ):
        raise CorpusFetchError("archive catalog references the wrong content lock")
    return directory, asset, size, digest


def _validated_zip_entries(
    archive: zipfile.ZipFile, directory: str, lock: dict[str, Any]
) -> list[zipfile.ZipInfo]:
    expected = _expected_archive_entries(directory, lock)
    infos = archive.infolist()
    names: list[str] = []
    folded: set[str] = set()
    for info in infos:
        name = info.filename
        path = PurePosixPath(name)
        if (
            info.is_dir()
            or "\\" in name
            or "\x00" in name
            or path.is_absolute()
            or not path.parts
            or path.parts[0] != directory
            or any(part in {"", ".", ".."} or ":" in part for part in path.parts)
        ):
            raise CorpusFetchError(f"unsafe or unexpected ZIP path: {name!r}")
        mode = (info.external_attr >> 16) & 0xFFFF
        if mode and stat.S_ISLNK(mode):
            raise CorpusFetchError(f"ZIP symlinks are forbidden: {name!r}")
        folded_name = name.casefold()
        if name in names or folded_name in folded:
            raise CorpusFetchError(f"duplicate or case-colliding ZIP path: {name!r}")
        names.append(name)
        folded.add(folded_name)
    if set(names) != expected:
        raise CorpusFetchError("ZIP entry set does not match the content lock")
    return infos


def install_archive(
    archive_path: Path,
    destination: Path,
    archive_spec: dict[str, Any],
    lock: dict[str, Any],
) -> Path:
    """Verify and atomically install one downloaded archive."""
    directory, _asset, expected_size, expected_digest = _validate_archive_spec(
        archive_spec, lock
    )
    size, digest = _sha256_file(archive_path)
    if size != expected_size or digest != expected_digest:
        raise CorpusFetchError(f"archive byte lock mismatch: {archive_path}")

    destination.mkdir(parents=True, exist_ok=True)
    if _is_link_or_reparse(destination):
        raise CorpusFetchError(f"corpus destination must not be a link or reparse point: {destination}")
    target = destination / directory
    if target.exists():
        verify_corpus(target, lock)
        return target

    with tempfile.TemporaryDirectory(prefix=f".{directory}-", dir=destination) as temporary:
        temporary_root = Path(temporary)
        with zipfile.ZipFile(archive_path, "r") as archive:
            infos = _validated_zip_entries(archive, directory, lock)
            for info in infos:
                relative = PurePosixPath(info.filename).relative_to(directory)
                output = temporary_root / directory / Path(*relative.parts)
                output.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(info, "r") as source, output.open("xb") as sink:
                    while chunk := source.read(_CHUNK_SIZE):
                        sink.write(chunk)
        extracted = temporary_root / directory
        verify_corpus(extracted, lock)
        os.replace(extracted, target)
    return target


def _download(url: str, path: Path, expected_size: int) -> None:
    request = urllib.request.Request(url, headers={"User-Agent": "mtg-kernel-corpus-fetch/1"})
    written = 0
    with urllib.request.urlopen(request, timeout=60) as response, path.open("xb") as sink:
        while chunk := response.read(_CHUNK_SIZE):
            written += len(chunk)
            if written > expected_size:
                raise CorpusFetchError(f"download exceeds its byte lock: {url}")
            sink.write(chunk)
    if written != expected_size:
        raise CorpusFetchError(f"download size does not match its byte lock: {url}")


def main(argv: list[str] | None = None) -> int:
    catalog, locks = _catalogs()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--destination",
        type=Path,
        default=REPO_ROOT / "corpora",
        help="directory in which corpus directories are installed",
    )
    parser.add_argument(
        "--corpus",
        action="append",
        dest="corpora",
        help="fetch only this corpus directory (repeatable; default: all)",
    )
    parser.add_argument(
        "--base-url",
        default=(
            f"https://github.com/{catalog['repository']}/releases/download/"
            f"{catalog['release_tag']}"
        ),
        help=argparse.SUPPRESS,
    )
    args = parser.parse_args(argv)
    requested = set(args.corpora or [])
    archives = catalog.get("archives")
    if not isinstance(archives, list) or not archives:
        raise CorpusFetchError("archive catalog has no archives list")
    validated_archives: list[
        tuple[dict[str, Any], dict[str, Any], str, str, int]
    ] = []
    available: set[str] = set()
    folded_directories: set[str] = set()
    folded_assets: set[str] = set()
    for archive_spec in archives:
        if not isinstance(archive_spec, dict):
            raise CorpusFetchError("malformed archive catalog record")
        candidate = archive_spec.get("directory")
        if not isinstance(candidate, str):
            raise CorpusFetchError("archive directory must be one safe path component")
        lock = locks.get(candidate)
        if lock is None:
            raise CorpusFetchError(f"missing content lock for {candidate!r}")
        directory, asset, expected_size, _digest = _validate_archive_spec(
            archive_spec, lock
        )
        folded_directory = directory.casefold()
        folded_asset = asset.casefold()
        if directory in available or folded_directory in folded_directories:
            raise CorpusFetchError(f"duplicate archive directory: {directory}")
        if folded_asset in folded_assets:
            raise CorpusFetchError(f"duplicate or case-colliding archive asset: {asset}")
        available.add(directory)
        folded_directories.add(folded_directory)
        folded_assets.add(folded_asset)
        validated_archives.append(
            (archive_spec, lock, directory, asset, expected_size)
        )
    if set(locks) != available:
        raise CorpusFetchError("archive and content-lock corpus sets differ")
    unknown = requested - available
    if unknown:
        raise CorpusFetchError(f"unknown corpus request: {sorted(unknown)!r}")

    args.destination.mkdir(parents=True, exist_ok=True)
    if _is_link_or_reparse(args.destination):
        raise CorpusFetchError(
            f"corpus destination must not be a link or reparse point: {args.destination}"
        )
    for archive_spec, lock, directory, asset, expected_size in validated_archives:
        if requested and directory not in requested:
            continue
        target = args.destination / directory
        if target.exists():
            verify_corpus(target, lock)
            print(f"CORPUS_FETCH: PASS existing {directory} -> {target}")
            continue
        url = f"{args.base_url.rstrip('/')}/{urllib.parse.quote(asset)}"
        fd, temporary_name = tempfile.mkstemp(prefix=f".{directory}-", suffix=".zip", dir=args.destination)
        os.close(fd)
        temporary_path = Path(temporary_name)
        temporary_path.unlink()
        try:
            _download(url, temporary_path, expected_size)
            installed = install_archive(temporary_path, args.destination, archive_spec, lock)
        finally:
            temporary_path.unlink(missing_ok=True)
        print(f"CORPUS_FETCH: PASS downloaded {directory} -> {installed}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except CorpusFetchError as exc:
        print(f"CORPUS_FETCH: FAIL: {exc}", file=os.sys.stderr)
        raise SystemExit(1) from exc
