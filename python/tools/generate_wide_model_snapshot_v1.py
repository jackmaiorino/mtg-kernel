"""Generate or verify the Python-authoritative wide (kernel-policy-value-net-8w128)
capacity-experiment model snapshot.

Small standalone verifier per the Capacity Experiment Contract (Stage 3): run
with ``--check`` to re-check the committed wide snapshot's digests without
touching the frozen common-model-snapshot path.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python"))

from mtg_kernel_rl.wide_model_snapshot_v1 import (  # noqa: E402
    wide_authority_check_v1,
    wide_portable_check_v1,
    write_wide_authority_snapshot_v1,
)


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_mutually_exclusive_group(required=True)
    commands.add_argument("--generate", action="store_true")
    commands.add_argument("--check", action="store_true")
    commands.add_argument("--authority-check", action="store_true")
    arguments = parser.parse_args()
    if arguments.generate:
        manifest, payload = write_wide_authority_snapshot_v1(REPO_ROOT)
        print(f"wrote {manifest}")
        print(f"wrote {payload}")
    elif arguments.authority_check:
        validated = wide_authority_check_v1(REPO_ROOT)
        print(f"wide model snapshot v1 authority check: PASS ({validated.manifest['integrity']['snapshot_sha256']})")
    else:
        validated = wide_portable_check_v1(REPO_ROOT)
        print(f"wide model snapshot v1 portable check: PASS ({validated.manifest['integrity']['snapshot_sha256']})")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
