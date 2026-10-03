"""Negative launch checks against real, completed storage qualification."""
import argparse
import copy
from pathlib import Path

from public_training_dispatch_v2 import read, write, pin
from public_training_storage_v1 import require_storage_choice


def run(compute, pilot):
    if not __debug__:
        raise RuntimeError("guard qualification requires Python checks enabled")
    root = compute/"guard-checks"
    root.mkdir()
    m = read(pilot/"manifest.json")
    original = read(compute/"compute-choice.json")
    qualification = read(compute/"qualification.json")
    checks = {}

    def reject(label, choice, binary=None):
        path = root/f"{label}.json"
        write(path, choice)
        try:
            require_storage_choice(path, binary or m["training_binary"], m["training_configs"])
        except (AssertionError, KeyError, ValueError, FileNotFoundError) as error:
            checks[label] = dict(rejected=True, error=type(error).__name__+": "+str(error))
        else:
            raise RuntimeError(f"invalid choice admitted: {label}")

    choice = copy.deepcopy(original)
    choice.pop("archive_scheme")
    reject("missing-storage-contract", choice)
    choice = copy.deepcopy(original)
    choice["selected"] = max(qualification["projections"], key=qualification["projections"].get)
    if choice["selected"] == original["selected"]:
        raise RuntimeError("need a measured slower alternative")
    reject("slower-allocation", choice)
    reject("changed-binary", original, dict(m["training_binary"], sha256="0"*64))
    for label, mutate in [
        ("mismatched-disk", lambda group: group["local_storage"].update(disk_serial="UNMEASURED")),
        ("missing-archive", lambda group: group.pop("archive")),
        ("unpriced-archive", lambda group: group.update(recovery_seconds=0)),
    ]:
        choice = copy.deepcopy(original)
        row = next(c for c in choice["candidates"] if c["id"] == choice["selected"])
        group = read(row["benchmark"]["path"])
        mutate(group)
        changed = root/f"{label}-group.json"
        write(changed, group)
        row["benchmark"] = pin(changed)
        reject(label, choice)
    selected = require_storage_choice(compute/"compute-choice.json", m["training_binary"], m["training_configs"])
    if selected != qualification["selected"]:
        raise RuntimeError("valid qualification readback mismatch")
    result = dict(complete=True, rejected_invalid_choices=checks, valid_choice_accepted=True,
        no_native_launches=True, checker=pin(__file__))
    write(compute/"guard-checks.json", result)
    print(result, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--compute", type=Path, required=True)
    parser.add_argument("--pilot", type=Path, required=True)
    args = parser.parse_args()
    run(args.compute, args.pilot)
