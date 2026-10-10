"""Stage the fixed diagnosis panel for one host root.

Usage: python stage_panel.py --panel PANEL.json --frozen DIR --sources DIR --root OUTROOT
         [--inputs HOSTDIR]
Writes OUTROOT/roots/<root_id>.jsonl (one frozen root each, verbatim lines of
the hash-checked frozen roots files) and OUTROOT/sources/*.json. With
--inputs, model and import paths in the sources are rewritten to files of the
same name under HOSTDIR (the sha256 pins are unchanged, so the loader still
verifies every file), and the files to copy are listed in OUTROOT/inputs.json.
"""
import argparse
import hashlib
import json
from pathlib import Path

FROZEN = {
    "r1": "fcc67b9afb0553e1ef7209e6d28fbb2c52145f960468b3a3b032ce9ff2b02539",
    "r2": "baab4ed3426c68af981e490645c45e9d38dbad87020c8641353d7bd01c1061af",
}
SOURCES = {
    "r1": "a63d24ea8305a378f2cf08878813c26481855519eb04e6e97afd8cd6c4405634",
    "r2": "edfed71da0d5ea2b6c707f2620bb95e3114531fa811dfb016960627205c89548",
    "t1": "8c237b85dc7863770e07b89d92c140fc54fef61aa68c527a6eeddc0e4e536ed4",
    "a48": "5d97f7c63857f9165db3d47c6c4a3b4b0dde01dfb13c4ed960da2171b2bda6de",
}


def sha(b):
    return hashlib.sha256(b).hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--panel", required=True)
    ap.add_argument("--frozen", required=True)
    ap.add_argument("--sources", required=True)
    ap.add_argument("--root", required=True)
    ap.add_argument("--inputs")
    a = ap.parse_args()
    root = Path(a.root)
    (root / "roots").mkdir(parents=True, exist_ok=True)
    (root / "sources").mkdir(parents=True, exist_ok=True)
    panel = json.loads(Path(a.panel).read_text())
    lines = {}
    for model, want in FROZEN.items():
        raw = (Path(a.frozen) / f"roots-{model}.jsonl").read_bytes()
        assert sha(raw) == want, f"roots-{model}.jsonl hash"
        for line in raw.decode("utf-8").splitlines():
            if line.strip():
                lines[json.loads(line)["root_id"]] = line
    for r in panel["roots"]:
        (root / "roots" / f"{r['root_id']}.jsonl").write_text(lines[r["root_id"]] + "\n", encoding="utf-8",
                                                               newline="\n")
    copies = []
    for name, want in SOURCES.items():
        raw = (Path(a.sources) / f"{name}-source.json").read_bytes()
        assert sha(raw) == want, f"{name}-source.json hash"
        src = json.loads(raw)
        if a.inputs:
            for key in ("checkpoint", "play_import"):
                old = src[key]["path"]
                new = f"{a.inputs.rstrip('/')}/{Path(old).name}"
                if key == "checkpoint" and name in ("r1", "r2"):
                    new = f"{a.inputs.rstrip('/')}/{name}-{Path(old).name}"
                copies.append({"from": old, "to": new, "sha256": src[key]["sha256"]})
                src[key]["path"] = new
            (root / "sources" / f"{name}-source.json").write_text(json.dumps(src, indent=1) + "\n")
        else:
            (root / "sources" / f"{name}-source.json").write_bytes(raw)
    if a.inputs:
        uniq = {c["to"]: c for c in copies}
        (root / "inputs.json").write_text(json.dumps(list(uniq.values()), indent=1) + "\n")
    print(f"staged {len(panel['roots'])} roots and {len(SOURCES)} sources under {root}")


if __name__ == "__main__":
    main()
