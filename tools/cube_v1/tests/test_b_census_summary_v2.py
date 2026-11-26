"""A2.5 eligible counts from the v2 drafted-census summary."""

from __future__ import annotations

import json
from pathlib import Path
import re
import sys
import tempfile
import unittest

CUBE_V1_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(CUBE_V1_DIR))

import b_census_summary as v1  # noqa: E402
import b_census_summary_v2 as v2  # noqa: E402


def line(index, kind, cards, source=None):
    end = {"kind": kind}
    return {
        "schema": v1.GAME_LINE_SCHEMA,
        "index": index,
        "end": end,
        "decks": [cards, cards[:1]],
        "completed": kind == "natural",
        "stop_kind": v1.stop_kind(end),
        "last_action_source": source,
    }


def test_exact_counts_include_hangs_and_exclude_unplayed_slots():
    rows = [
        line(0, "natural", [1, 4]),
        line(1, "decision_cap", [1, 4], 4),
        line(2, "lost", [1, 2, 4], 4),
        line(3, "hang", [1, 4], 4),
        line(4, "unrun", [1, 2, 4], 4),
        line(5, "process_died", [1, 2, 4], 4),
        line(6, "panic", [1, 4], 4),
    ]
    summary = v2.summarise(rows)
    assert summary.pop("tripwire") == {
        "schema": v2.ELIGIBLE_SCHEMA,
        "games": 4,
        "cards": [
            {"card": 1, "games_present": 4, "charged": 0},
            {"card": 4, "games_present": 4, "charged": 2},
        ],
    }
    assert summary == v1.summarise(rows)


def test_v1_summary_cannot_distinguish_hidden_lost_card_presence():
    def rows(extra):
        return [
            line(i, "lost" if i < 4 else "decision_cap", list(range(1, 6)) +
                 ([21] if i in (6, extra) else []), 21 if i == 6 else None)
            for i in range(8)
        ]

    assert v1.summarise(rows(0)) == v1.summarise(rows(5))
    card_a = v2.summarise(rows(0))["tripwire"]["cards"][-1]
    card_b = v2.summarise(rows(5))["tripwire"]["cards"][-1]
    assert (card_a["charged"], card_a["games_present"]) == (1, 1)
    assert (card_b["charged"], card_b["games_present"]) == (1, 2)


def test_cli_reconciles_only_declared_missing_slots(tmp_path):
    games = tmp_path / "games.jsonl"
    games.write_text(
        "\n".join(json.dumps(row) for row in [line(0, "natural", [1]), line(3, "hang", [2], 2)]),
        encoding="utf-8",
    )
    out = tmp_path / "summary.json"
    slot_file = tmp_path / "slots.json"
    slot_file.write_text('{"lost":[1],"unrun":[2]}', encoding="utf-8")
    args = ["--games", str(games), "--out", str(out)]
    assert v2.main(args) == 1
    assert v2.main(args + ["--excluded-slots", str(slot_file)]) == 0
    summary = json.loads(out.read_bytes())
    assert summary["order_errors"] == []
    assert summary["tripwire"]["games"] == 2
    assert [row["stop_kind"] for row in summary["stop_kinds"]] == ["hang", "lost", "unrun"]


def test_cli_reports_overlap_between_game_and_excluded_slot(tmp_path):
    games = tmp_path / "games.jsonl"
    games.write_text(json.dumps(line(0, "natural", [1])), encoding="utf-8")
    slots = tmp_path / "slots.json"
    slots.write_text('{"lost":[0]}', encoding="utf-8")
    out = tmp_path / "summary.json"
    assert v2.main(["--games", str(games), "--out", str(out), "--excluded-slots", str(slots)]) == 1
    assert json.loads(out.read_bytes())["order_errors"]


def test_rejects_conflicting_slot_classes(tmp_path):
    slots = tmp_path / "slots.json"
    slots.write_text('{"lost":[1],"unrun":[1]}', encoding="utf-8")
    try:
        list(v2.read_excluded_slots(slots))
    except ValueError as error:
        assert re.search("duplicate excluded slot", str(error))
    else:
        raise AssertionError("conflicting slot classes were accepted")


class SummaryV2Tests(unittest.TestCase):
    """Run the module's test functions under unittest discovery, as CI does."""

    def _run(self, test, needs_tmp):
        if needs_tmp:
            with tempfile.TemporaryDirectory() as tmp:
                test(Path(tmp))
        else:
            test()

    def test_exact_counts(self):
        self._run(test_exact_counts_include_hangs_and_exclude_unplayed_slots, False)

    def test_v1_cannot_distinguish(self):
        self._run(test_v1_summary_cannot_distinguish_hidden_lost_card_presence, False)

    def test_cli_reconciles(self):
        self._run(test_cli_reconciles_only_declared_missing_slots, True)

    def test_cli_reports_overlap(self):
        self._run(test_cli_reports_overlap_between_game_and_excluded_slot, True)

    def test_rejects_conflicts(self):
        self._run(test_rejects_conflicting_slot_classes, True)


if __name__ == "__main__":
    unittest.main()
