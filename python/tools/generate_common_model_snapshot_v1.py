"""Generate or verify the Python-authoritative common model snapshot v1."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python"))

from mtg_kernel_rl.common_model_snapshot_v1 import (  # noqa: E402
    authority_check_v1,
    portable_check_v1,
    write_authority_snapshot_v1,
)


def main() -> int:
    parser = argparse.ArgumentParser()
    commands = parser.add_mutually_exclusive_group(required=True)
    commands.add_argument("--generate", action="store_true")
    commands.add_argument("--check", action="store_true")
    commands.add_argument("--authority-check", action="store_true")
    arguments = parser.parse_args()
    if arguments.generate:
        manifest, payload = write_authority_snapshot_v1(REPO_ROOT)
        print(f"wrote {manifest}")
        print(f"wrote {payload}")
    elif arguments.authority_check:
        authority_check_v1(REPO_ROOT)
        print("common model snapshot v1 authority check: PASS")
    else:
        portable_check_v1(REPO_ROOT)
        print("common model snapshot v1 portable check: PASS")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
