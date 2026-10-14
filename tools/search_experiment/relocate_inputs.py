"""Rebase an uploaded explicit input index onto its verified Linux directory."""
import argparse
from pathlib import Path, PureWindowsPath
import shutil

from launch import checked, digest, load, publish, require


def relocate(inputs):
    inputs = Path(inputs).resolve()
    index = load(inputs / "INPUTS.json")
    original = inputs / "INPUTS-TRANSPORT.json"
    require(not original.exists(), "input relocation already recorded")
    shutil.copy2(inputs / "INPUTS.json", original)
    assets = {}
    source_names = set(index["sources"])
    for asset in index["assets"]:
        name = PureWindowsPath(asset["path"]).name
        if name in source_names:
            continue
        target = inputs / ("objects" if "objects" in PureWindowsPath(asset["path"]).parts else "") / name
        checked({"path": str(target), "sha256": asset["sha256"]})
        assets[str(target)] = {**asset, "transport_path": asset["path"], "path": str(target)}
    for name, record in index["sources"].items():
        source = load(inputs / name)
        require(digest(inputs / name) == record["sha256"], "uploaded source specification differs")
        leaves = []
        for key in ("checkpoint", "play_import"):
            leaf = source[key]
            target = inputs / "objects" / (leaf["sha256"] + ".json")
            checked({"path": str(target), "sha256": leaf["sha256"]})
            leaf["path"] = str(target)
            leaves.append({"path": str(target), "sha256": leaf["sha256"]})
        target = inputs / name
        publish(target, source)
        record.update(transport_sha256=record["sha256"], path=str(target), sha256=digest(target), leaves=leaves)
        assets[str(target)] = {"path": str(target), "sha256": record["sha256"]}
    index.update(assets=list(assets.values()), transport_index={"path": str(original), "sha256": digest(original)})
    publish(inputs / "INPUTS.json", index)
    return {"assets": len(assets), "transport_index": index["transport_index"], "relocated": True}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--inputs", required=True)
    args = parser.parse_args()
    print(relocate(args.inputs))
