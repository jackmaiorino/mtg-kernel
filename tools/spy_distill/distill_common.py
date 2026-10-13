"""Shared constants and helpers of the Spy archive distillation screen.

collab LANES/spy-archive-distillation-20261013/DESIGN.md (v3, frozen). The
schedules reuse tools/spy_reset/pilot.py (spy-reset-pilot-v1): its seed
encoding, decks, limits, runtime rules and opponent rule by j.
"""
from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools" / "spy_reset"))
import pilot  # noqa: E402

NS = "spy-archive-distillation-v1"
ANCHOR_NS = "spy-archive-distillation-anchor-v1"
OFFERS_NS = "spy-archive-distillation-offers-v1"
LANE = "claude-spy-archive-distillation-20261013"
LINEAGES = pilot.LINEAGES
DECKS, SPY = pilot.DECKS, pilot.SPY
# Arm letter -> the engine's ArchiveDistillationArmV1 wire name.
ARMS = {"U": "uniform", "L": "line_decisions"}
ARM_LETTERS = {v: k for k, v in ARMS.items()}
# The named fixed opponent that plays O's "initial" units in phase 2.
PARENT_ID = "parent"
# The parents' and O's final checkpoint files (archive reset pilot evidence/index.json).
PARENT_CHECKPOINT_SHA256 = {"r1": "5c104b0d5bb532568549e9b2bdf6f787d32e51652b7154a148ffc94213c1a50b",
                            "r2": "d61079228505aac620302e3e5e608020f4ac4790343ea6a48979e4b10603da99"}
O_FINAL_CHECKPOINT_SHA256 = {"r1": "b81ce5c5ba3f98426f1188affb4d486847f6b78b69172467e13adde961dd14de",
                             "r2": "4cbe46eccdbf0cc303b95e2eb298fec283fe789e2d352d41a0e078daebb9eb91"}
# Collections use the frozen trainer's tolerance (ndb.FROZEN), so the
# parent's own play is collected under the rules it trained under.
NON_NATURAL_FRACTION = pilot.ndb.FROZEN["max_non_natural_episode_fraction"]


def check_lineage(lineage: str) -> None:
    pilot.check_lineage(lineage)


def check_parent(lineage: str, parent: dict) -> None:
    """The lineage's parent source: its checkpoint is the parent the archive
    pilot trained O from."""
    check_lineage(lineage)
    require((parent.get("checkpoint") or {}).get("sha256") == PARENT_CHECKPOINT_SHA256[lineage],
            f"the source is not the {lineage} parent's checkpoint")


def collection_opponent(j: int) -> str:
    """The pilot's rule by j, collected from the parent: 0 to 7 the parent
    (current and initial are both the parent), 8 and 9 the fixed A48."""
    return "a48" if pilot.opponent_kind(j % 10) == "fixed" else "parent"


def pin(path) -> dict:
    """A PinnedFileV1 (path and SHA-256 of the bytes, nothing else)."""
    path = Path(path)
    return {"path": str(path).replace("\\", "/"), "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}


def read_json(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def write_json(path, value) -> dict:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, separators=(",", ":")) + "\n", encoding="utf-8")
    return pin(path)


def read_jsonl(path):
    with Path(path).open(encoding="utf-8") as handle:
        for line in handle:
            if line.strip():
                yield json.loads(line)


def episode(eid: str, seed: int, own: int, other: int, seat: int, starting_player: int,
            decks: list[dict], opponent: dict | None = None) -> dict:
    """An ExpandedEpisodeV1 on the pilot's decks, limits and runtime rules."""
    seats = [decks[own], decks[other]] if seat == 0 else [decks[other], decks[own]]
    value = {"id": eid, "seed": seed, "starting_player": starting_player, "learner_seat": seat,
             "registered": [dict(d) for d in seats], "selected": [dict(d) for d in seats],
             "postboard": False, "max_physical_decisions": pilot.MAX_PHYSICAL_DECISIONS,
             "max_policy_steps": pilot.MAX_POLICY_STEPS, "runtime_rules": pilot.RUNTIME_RULES}
    if opponent is not None:
        value["opponent"] = opponent
    return value


def require(condition, message: str) -> None:
    if not condition:
        raise ValueError(message)
