"""Stage the registered nine-root input panel from explicit manifest paths."""
import argparse
import json
from pathlib import Path
import shutil
import subprocess

from launch import GIB, checked, digest, publish, require


def git_blob(repo, ref, path):
    return subprocess.check_output(["git", "-C", str(repo), "show", f"{ref}:{path}"])


def stage(repo, ref, root, sources):
    ref = subprocess.check_output(["git", "-C", str(repo), "rev-parse", ref], text=True).strip()
    root = Path(root)
    require(root.is_dir(), "create the owned SSD root first")
    require(shutil.disk_usage(root).free >= 60 * GIB + 128 * 1024**2, "SSD reserve insufficient")
    manifest_path = "LANES/search-efficiency-20261013/MANIFEST.json"
    source_manifest = json.loads(git_blob(repo, ref, manifest_path))
    inputs = root / "inputs"
    inputs.mkdir()
    objects = inputs / "objects"
    objects.mkdir()
    records = []
    for name, expected in source_manifest["inputs"]["files"].items():
        data = git_blob(repo, ref, f"LANES/search-efficiency-20261013/evidence/inputs/{name}")
        target = inputs / name
        target.write_bytes(data)
        require(digest(target) == expected, f"frozen root input differs: {name}")
        records.append({"path": str(target), "sha256": expected, "origin": f"{ref}:{name}"})
    expected_sources = json.loads(git_blob(repo, ref, "LANES/search-total-budget-20261011/MANIFEST.json"))["inputs"]["sources"]
    for lineage, expected in source_manifest["inputs"]["bank_specs"].items():
        if lineage not in ("r1", "r2"):
            continue
        original = Path(sources).parent / "frozen" / f"bank-spec-{lineage}.json"
        target = inputs / original.name
        shutil.copy2(checked({"path": str(original), "sha256": expected}), target)
        records.append({"path": str(target), "sha256": expected, "origin": str(original)})
    source_records = {}
    for name, expected in expected_sources.items():
        original = Path(sources) / name
        source = json.loads(checked({"path": str(original), "sha256": expected}).read_bytes())
        leaves = []
        for key in ("checkpoint", "play_import"):
            leaf = source[key]
            original_leaf = checked(leaf)
            target = objects / (leaf["sha256"] + ".json")
            if not target.exists():
                shutil.copy2(original_leaf, target)
            require(digest(target) == leaf["sha256"], "staged leaf differs")
            leaves.append({"path": str(target), "sha256": leaf["sha256"], "origin": str(original_leaf)})
            leaf["path"] = target.as_posix()
        target = inputs / name
        publish(target, source)
        source_records[name] = {"path": str(target), "sha256": digest(target),
                                "origin": str(original), "origin_sha256": expected, "leaves": leaves}
        records.extend(leaves)
        records.append({"path": str(target), "sha256": digest(target)})
    record = {"collab_ref": ref,
              "root_panel_origin": manifest_path, "sources": source_records,
              "assets": list({r["path"]: r for r in records}.values()),
              "bytes": sum(p.stat().st_size for p in inputs.rglob("*") if p.is_file())}
    publish(inputs / "INPUTS.json", record)
    return record


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--collab", required=True)
    parser.add_argument("--ref", default="origin/main")
    parser.add_argument("--root", required=True)
    parser.add_argument("--sources", default="D:/budget-screen-20261011/sources")
    args = parser.parse_args()
    print(json.dumps(stage(args.collab, args.ref, args.root, args.sources)))
