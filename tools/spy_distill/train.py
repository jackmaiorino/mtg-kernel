"""Phase-2 and O-reproduction training runs through the guarded launcher.

tools/spy_reset/train_driver.py unchanged (runtime, requests, wait, choice,
produce, dispatch), under this screen's lane name. Same CLI.
"""
from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "spy_reset"))
sys.path.insert(0, str(Path(__file__).resolve().parent))
import train_driver  # noqa: E402
from distill_common import LANE  # noqa: E402

train_driver.LANE = LANE

if __name__ == "__main__":
    raise SystemExit(train_driver.main())
