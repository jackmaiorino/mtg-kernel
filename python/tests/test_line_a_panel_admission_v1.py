"""Tests for python/tools/line_a_panel_admission_v1.py: the R14 acceptance references (CODEX #570, #573).

An R14 admission receipt needs an exact Fable countersign section and Codex's affirmative implementation
acceptance line; headings, pending or negative notes, and commits mentioned in discussion never count.
"""
from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import line_a_panel_admission_v1 as admission  # noqa: E402

COMMIT = "0cd213dd7f0ab8ff4a694e5e9944007eff329a24"
OTHER = "5dacb01b12dce9c8c5aa963137d342b017fb40d1"
HEADING = ("Opus lane panel-export: R14 registry-evolution import route (design and implementation review): "
           "COUNTERSIGN (scoped): the design is countersigned subject to bounded changes")
CHANGE_REQUIRED = "Opus lane panel-export: R14 addendum: CHANGE-REQUIRED: amend the loader first"
FABLE = f"# Fable review record\n\n## {HEADING}\n\nVerdict body.\n\n## {CHANGE_REQUIRED}\n\nBody.\n\n## Window notes\n"


def note(number: int, heading: str, *body: str) -> str:
    return f"## CODEX #{number} [2026-09-30 12:00 EDT] CODEX -> OPUS PANEL-EXPORT: {heading}\n\n" + "\n".join(body) + "\n"


class AcceptanceReferenceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)

    def tearDown(self) -> None:
        self.tmp.cleanup()

    def mailbox(self, *notes: str) -> Path:
        path = self.root / "TO-CODEX.md"
        path.write_text("# Mailbox\n\n" + "\n".join(notes), encoding="utf-8")
        return path

    def refused(self, call, *args) -> str:
        with self.assertRaises(SystemExit) as caught:
            call(*args)
        message = str(caught.exception)
        self.assertTrue(message.startswith("admission refused: "), message)
        return message

    def test_the_exact_acceptance_line_is_accepted(self) -> None:
        box = self.mailbox(note(900, "R14 IMPLEMENTATION ACCEPTED", "Tests rerun.", admission.acceptance_line(COMMIT)))
        result = admission.codex_reference("CODEX #900", COMMIT, box)
        self.assertTrue(result["heading"].startswith("CODEX #900 ["))
        self.assertEqual(len(result["note_sha256"]), 64)

    def test_pending_and_negative_notes_are_refused(self) -> None:
        box = self.mailbox(
            note(901, "R14 IMPLEMENTATION COUNTERSIGN PENDING", f"Reviewed {COMMIT}; countersign after the rerun."),
            note(902, "NO COUNTERSIGN", "R14 implementation acceptance: PENDING"),
            note(903, "R14 IMPLEMENTATION COUNTERSIGNED", admission.acceptance_line(COMMIT),
                 "R14 implementation acceptance: WITHDRAWN"),
        )
        for number in (901, 902, 903):
            self.refused(admission.codex_reference, f"CODEX #{number}", COMMIT, box)

    def test_a_commit_mentioned_only_in_discussion_is_refused(self) -> None:
        box = self.mailbox(note(904, "R14 IMPLEMENTATION COUNTERSIGNED", f"Tests pass at {COMMIT}.",
                                "Fable COUNTERSIGN (scoped) is recorded."))
        self.refused(admission.codex_reference, "CODEX #904", COMMIT, box)

    def test_the_acceptance_line_must_match_exactly(self) -> None:
        exact = admission.acceptance_line(COMMIT)
        variants = ["`" + exact + "`", "- " + exact, exact + " (tests rerun)", " " + exact,
                    admission.acceptance_line(COMMIT[:8]), admission.acceptance_line(OTHER),
                    exact.replace("COUNTERSIGN", "countersign")]
        for number, line in enumerate(variants, start=910):
            box = self.mailbox(note(number, "R14 IMPLEMENTATION ACCEPTED", line))
            self.refused(admission.codex_reference, f"CODEX #{number}", COMMIT, box)

    def test_only_the_identified_note_counts(self) -> None:
        box = self.mailbox(note(920, "DISCUSSION", f"Reviewing {COMMIT}."),
                           note(921, "R14 IMPLEMENTATION ACCEPTED", admission.acceptance_line(COMMIT)))
        self.refused(admission.codex_reference, "CODEX #920", COMMIT, box)
        admission.codex_reference("CODEX #921", COMMIT, box)

    def test_missing_duplicate_and_malformed_notes_are_refused(self) -> None:
        line = admission.acceptance_line(COMMIT)
        box = self.mailbox(note(930, "ACCEPTED", line), note(930, "ACCEPTED AGAIN", line))
        self.refused(admission.codex_reference, "CODEX #930", COMMIT, box)
        self.refused(admission.codex_reference, "CODEX #931", COMMIT, box)
        for reference in ("Codex #930", "CODEX #", "CODEX #93a", "CODEX #930 "):
            self.refused(admission.codex_reference, reference, COMMIT, box)

    def test_the_exact_fable_heading_is_accepted(self) -> None:
        record = self.root / "FABLE-REVIEW-20260927.md"
        record.write_text(FABLE, encoding="utf-8")
        reference, evidence = admission.fable_reference(record, HEADING)
        self.assertEqual(reference, "FABLE-REVIEW-20260927.md#" + HEADING)
        self.assertEqual(len(evidence["section_sha256"]), 64)

    def test_heading_prefixes_other_verdicts_and_other_files_are_refused(self) -> None:
        record = self.root / "FABLE-REVIEW-20260927.md"
        record.write_text(FABLE, encoding="utf-8")
        self.refused(admission.fable_reference, record, "Opus lane panel-export: R14 registry-evolution import route")
        self.refused(admission.fable_reference, record, CHANGE_REQUIRED)
        self.refused(admission.fable_reference, record, HEADING + " ")
        other = self.root / "review.md"
        other.write_text(FABLE, encoding="utf-8")
        self.refused(admission.fable_reference, other, HEADING)

    def test_the_accepted_commit_must_carry_the_source_card_db_pin(self) -> None:
        present = all(subprocess.run(["git", "-C", str(REPO_ROOT), "cat-file", "-e", c + "^{commit}"],
                                     capture_output=True).returncode == 0 for c in (COMMIT, OTHER))
        if not present:
            self.skipTest("the pinned lane commits are not in this clone")
        admission.commit_reference(COMMIT)
        self.refused(admission.commit_reference, OTHER)
        self.refused(admission.commit_reference, COMMIT.upper())
        self.refused(admission.commit_reference, COMMIT[:8])


if __name__ == "__main__":
    unittest.main()
