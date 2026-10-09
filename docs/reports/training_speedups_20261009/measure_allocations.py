"""Read-only Windows NTFS allocation measurement and empirical block projection.

No content hashing, deletion, compression change, remote execution or launch.
Run on the storage host after a complete one-update qualification and recovery.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import sys

GIB = 1024 ** 3
HISTORICAL_FULL_BLOCK_ZIP = 3_498_824_608


def require(ok, message):
    if not ok:
        raise ValueError(message)


def file_api():
    require(os.name == "nt", "GetCompressedFileSizeW requires Windows")
    dll = ctypes.WinDLL("kernel32", use_last_error=True)
    function = dll.GetCompressedFileSizeW
    function.argtypes = [wintypes.LPCWSTR, ctypes.POINTER(wintypes.DWORD)]
    function.restype = wintypes.DWORD
    def allocated(path):
        # Long paths preserve native Win32 semantics. UNC is supported too.
        name = str(path.absolute())
        if not name.startswith("\\\\?\\"):
            name = "\\\\?\\UNC\\" + name[2:] if name.startswith("\\\\") else "\\\\?\\" + name
        high = wintypes.DWORD()
        ctypes.set_last_error(0)
        low = function(name, ctypes.byref(high))
        error = ctypes.get_last_error()
        if low == 0xFFFFFFFF and error:
            raise ctypes.WinError(error)
        return (high.value << 32) | low
    return allocated


def reparse(meta):
    return bool(getattr(meta, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT)


def check_parents(path):
    for parent in (path, *path.parents):
        meta = parent.lstat()
        require(not reparse(meta) and not stat.S_ISLNK(meta.st_mode), "root traverses reparse point: " + str(parent))


def classify(path, native_root):
    if path.suffix.lower() == ".zip":
        return "zip"
    if path.is_relative_to(native_root) or "native" in path.parts or "matched-native" in path.parts:
        return "raw_native"
    if path.suffix.lower() in (".json", ".jsonl", ".log", ".txt", ".md", ".csv"):
        return "receipts"
    return "other"


def measure(root, native_root, allocation, on_reparse):
    classes = {name: {"files": 0, "logical_bytes": 0, "allocated_bytes": 0,
                      "compressed_files": 0, "uncompressed_files": 0}
               for name in ("raw_native", "zip", "receipts", "other")}
    files, skipped, errors = [], [], []
    stack = [root]
    representative = {"files": 0, "logical_bytes": 0, "allocated_bytes": 0}
    while stack:
        directory = stack.pop()
        try:
            directory_meta = directory.lstat()
            if reparse(directory_meta) or stat.S_ISLNK(directory_meta.st_mode):
                skipped.append(str(directory))
                continue
            entries = list(os.scandir(directory))
        except OSError as error:
            errors.append({"path": str(directory), "error": str(error)})
            continue
        for entry in entries:
            path = Path(entry.path)
            try:
                before = entry.stat(follow_symlinks=False)
                if reparse(before) or stat.S_ISLNK(before.st_mode):
                    skipped.append(str(path))
                    continue
                if stat.S_ISDIR(before.st_mode):
                    stack.append(path)
                    continue
                require(stat.S_ISREG(before.st_mode), "nonregular entry")
                physical = allocation(path)
                after = path.lstat()
                require(not reparse(after) and not stat.S_ISLNK(after.st_mode), "file became a reparse point")
                require((before.st_size, before.st_mtime_ns, before.st_ino) ==
                        (after.st_size, after.st_mtime_ns, after.st_ino), "file changed while measuring")
                category = classify(path, native_root)
                compressed = bool(before.st_file_attributes & stat.FILE_ATTRIBUTE_COMPRESSED)
                aggregate = classes[category]
                aggregate["files"] += 1
                aggregate["logical_bytes"] += before.st_size
                aggregate["allocated_bytes"] += physical
                aggregate["compressed_files" if compressed else "uncompressed_files"] += 1
                if path.is_relative_to(native_root):
                    representative["files"] += 1
                    representative["logical_bytes"] += before.st_size
                    representative["allocated_bytes"] += physical
                files.append({"path": str(path), "class": category, "logical_bytes": before.st_size,
                              "allocated_bytes": physical, "mtime_ns": before.st_mtime_ns,
                              "compressed_attribute": compressed})
            except (OSError, ValueError) as error:
                errors.append({"path": str(path), "error": str(error)})
    files.sort(key=lambda row: row["path"].lower())
    metadata_digest = hashlib.sha256(json.dumps(files, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    return {"classes": classes, "files": files, "representative_native": representative,
            "skipped_reparse_points": skipped, "errors": errors,
            "complete": not errors and (not skipped or on_reparse == "skip"),
            "metadata_snapshot_sha256": metadata_digest,
            "metadata_snapshot_semantics": "Digest of paths, logical/allocation sizes, mtimes and compression attributes; not content integrity."}


def projection(raw, updates, observed_updates):
    # Integer ceilings avoid understating a byte projection.
    extrapolated = (raw * updates + observed_updates - 1) // observed_updates
    base = extrapolated + HISTORICAL_FULL_BLOCK_ZIP
    return {"raw_extrapolated_bytes": extrapolated,
            "historical_zip_assumed_incompressible_bytes": HISTORICAL_FULL_BLOCK_ZIP,
            "base_bytes": base, "margin_fraction": 0.25,
            "extra_bytes": 512 * 1024 ** 2,
            "projected_peak_additional_bytes": (base * 125 + 99) // 100 + 512 * 1024 ** 2}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="Existing accounting tree")
    parser.add_argument("--native-root", type=Path, required=True, help="One completed representative qualification native tree")
    parser.add_argument("--observed-updates", type=int, default=1)
    parser.add_argument("--observed-games", type=int, default=10)
    parser.add_argument("--full-updates", type=int, default=162)
    parser.add_argument("--reserve-bytes", type=int, default=60 * GIB)
    parser.add_argument("--on-reparse", choices=("fail", "skip"), default="fail")
    args = parser.parse_args()
    require(args.root.is_absolute() and args.native_root.is_absolute(), "absolute roots required")
    require(args.root.is_dir() and args.native_root.is_dir(), "existing roots required")
    check_parents(args.root)
    check_parents(args.native_root)
    root, native_root = args.root.absolute(), args.native_root.absolute()
    require(native_root.is_relative_to(root), "representative native root must be within accounting tree")
    require(args.observed_updates == 1 and args.observed_games == 10 and args.full_updates == 162,
            "this calibration expects one complete 10-game update and a 162-update block")
    require(args.reserve_bytes >= 60 * GIB, "reserve cannot be below 60 GiB")
    result = measure(root, native_root, file_api(), args.on_reparse)
    raw = result["representative_native"]
    require(raw["files"] > 0, "representative native tree is empty")
    volume = shutil.disk_usage(root)
    physical = projection(raw["allocated_bytes"], args.full_updates, args.observed_updates)
    logical = projection(raw["logical_bytes"], args.full_updates, args.observed_updates)
    headroom = max(0, volume.free - args.reserve_bytes)
    trustworthy = result["complete"] and not result["skipped_reparse_points"]
    result.update({"schema": "training-speedups-allocation-calibration/v1", "root": str(root),
                   "native_root": str(native_root), "observed_updates": args.observed_updates,
                   "observed_games": args.observed_games, "projected_games": args.full_updates * args.observed_games,
                   "volume": {"drive": root.drive, "free_bytes": volume.free, "total_bytes": volume.total,
                              "reserve_bytes": args.reserve_bytes, "free_above_reserve_bytes": headroom},
                   "physical_projection": physical, "logical_projection": logical,
                   "empirical_physical_headroom_admits_projection": trustworthy and physical["projected_peak_additional_bytes"] <= headroom,
                   "projection_kind": "Empirical first-update extrapolation with 25 percent margin plus 512 MiB, not an exact bound.",
                   "limitations": ["Later trajectory lengths and checkpoint sizes can differ from update one.",
                                    "ZIP projection uses observed historical whole-block ZIP bytes as incompressible.",
                                    "Existing retained files reduce current free space; no files are removed.",
                                    "Win32 file allocation excludes directory metadata and unrelated concurrent writes.",
                                    "Skipped reparse entries or measurement errors prevent an affirmative admission result.",
                                    "A physical fit does not satisfy or change the supported launcher's conservative projection guard."]})
    print(json.dumps(result, indent=2))
    return 0 if trustworthy else 2

if __name__ == "__main__":
    try:
        sys.exit(main())
    except (ValueError, OSError) as error:
        print(json.dumps({"complete": False, "error": str(error)}), file=sys.stderr)
        sys.exit(2)
