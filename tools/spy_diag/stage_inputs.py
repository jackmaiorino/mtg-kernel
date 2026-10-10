"""Collect the model inputs a staged host root needs into one directory.

Usage: python stage_inputs.py --root STAGEROOT --out DIR --host-inputs HOSTDIR
Reads STAGEROOT/inputs.json (from stage_panel.py --inputs). Copies each
checkpoint verbatim (hash-checked). Each play import names further objects by
absolute path; those objects are copied verbatim and the import is rewritten
to name HOSTDIR, and the staged sources are re-pinned to the rewritten
import's hash. Writes DIR/MAPPING.json (original path and hash to host path
and hash) for the run manifest.
"""
import argparse
import hashlib
import json
import shutil
from pathlib import Path


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--host-inputs", required=True)
    a = ap.parse_args()
    root, out, host = Path(a.root), Path(a.out), a.host_inputs.rstrip("/")
    out.mkdir(parents=True, exist_ok=True)
    mapping = []
    import_pins = {}
    for c in json.loads((root / "inputs.json").read_text()):
        src = Path(c["from"])
        assert sha(src) == c["sha256"], c
        name = Path(c["to"]).name
        if name.endswith("-import.json"):
            body = json.loads(src.read_text())
            for key in ("initialization", "parameters"):
                old = body[key]["path"]
                assert sha(old) == body[key]["sha256"], old
                shutil.copyfile(old, out / Path(old).name)
                body[key]["path"] = f"{host}/{Path(old).name}"
                mapping.append({"from": old, "to": body[key]["path"], "sha256": body[key]["sha256"]})
            text = json.dumps(body, separators=(",", ":"), sort_keys=True)
            (out / name).write_text(text, encoding="utf-8", newline="\n")
            new = sha(out / name)
            import_pins[c["to"]] = new
            mapping.append({"from": str(src), "to": c["to"], "sha256": c["sha256"], "host_sha256": new,
                            "rewritten": "object paths only"})
        else:
            shutil.copyfile(src, out / name)
            mapping.append({"from": str(src), "to": c["to"], "sha256": c["sha256"]})
    for f in (root / "sources").glob("*-source.json"):
        s = json.loads(f.read_text())
        if s["play_import"]["path"] in import_pins:
            s["play_import"]["sha256"] = import_pins[s["play_import"]["path"]]
        f.write_text(json.dumps(s, indent=1) + "\n")
    (out / "MAPPING.json").write_text(json.dumps(mapping, indent=1) + "\n")
    print(f"{len(mapping)} files staged in {out}")


if __name__ == "__main__":
    main()
