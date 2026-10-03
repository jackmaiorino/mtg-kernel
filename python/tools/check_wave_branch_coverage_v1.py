#!/usr/bin/env python3
"""Check a kernel wave branch-coverage manifest against `covers:` test
annotations.

A wave branch manifest (`data/wave_branch_manifests/<wave>.json`) declares,
per card, the observable rules branches its wave is expected to exercise in
the Rust test suite. This tool scans `*.rs` files under the given roots for
`covers:` line comments immediately above a `#[test]` attribute, binds each
declared branch to the test function that exercises it, and fails closed
(exit 1) if any declared branch is unexercised, or if an annotation names a
card or branch the manifest does not declare (typos must fail, never
silently pass).

When several waves' manifests coexist, the scan roots hold every wave's
annotations at once, so pass each other wave's manifest with
`--also-declared` to say "declared, but not this wave's to check". A card
name that appears in no supplied manifest still fails closed.

This tool is intentionally stdlib-only.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable

REPORT_SCHEMA = "kernel_wave_branch_coverage_report/v1"

_COVERS_RE = re.compile(r"^\s*//\s*covers:\s*(.+)$")
_TEST_ATTR_RE = re.compile(r"^\s*#\[test\]\s*$")
_ATTR_RE = re.compile(r"^\s*#\[.*\]\s*$")
_FN_RE = re.compile(r"^\s*(?:pub\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)")


@dataclass(frozen=True)
class Binding:
    card: str
    branch: str
    file: str  # posix-style path, relative to the scan root's parent when possible
    line: int
    test: str


@dataclass(frozen=True)
class ContiguityDiagnostic:
    """A `covers:`/`#[test]` structural problem, reported in addition to
    (never instead of) the resulting unexercised-branch failure."""

    kind: str  # "orphaned_covers" or "test_without_fn"
    file: str
    line: int
    message: str


@dataclass
class CardManifest:
    name: str
    post_board: bool
    branches: list[str]
    unreachable: dict[str, str] = field(default_factory=dict)


class ManifestError(RuntimeError):
    """Raised when the manifest itself is malformed."""


def load_manifest(path: Path) -> tuple[str, dict[str, CardManifest]]:
    try:
        raw = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise ManifestError(f"cannot read manifest {path}: {exc}") from exc
    if raw.get("schema") != "kernel_wave_branch_manifest/v1":
        raise ManifestError(
            f"{path}: unexpected schema {raw.get('schema')!r}, "
            "expected kernel_wave_branch_manifest/v1"
        )
    wave = raw.get("wave", "")
    cards: dict[str, CardManifest] = {}
    for name, entry in raw.get("cards", {}).items():
        branches = list(entry.get("branches", []))
        if len(set(branches)) != len(branches):
            raise ManifestError(f"{path}: card {name!r} declares duplicate branch names")
        unreachable = dict(entry.get("unreachable", {}))
        overlap = set(branches) & set(unreachable)
        if overlap:
            raise ManifestError(
                f"{path}: card {name!r} lists {sorted(overlap)} in both "
                "branches and unreachable"
            )
        cards[name] = CardManifest(
            name=name,
            post_board=bool(entry.get("post_board", False)),
            branches=branches,
            unreachable=unreachable,
        )
    return wave, cards


def _display_path(path: Path) -> str:
    try:
        return path.resolve().relative_to(Path.cwd().resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def parse_file(path: Path) -> tuple[list[Binding], list[ContiguityDiagnostic]]:
    text = path.read_text(encoding="utf-8")
    lines = text.splitlines()
    bindings: list[Binding] = []
    diagnostics: list[ContiguityDiagnostic] = []
    display = _display_path(path)

    # Every `covers:`-shaped line in the file, independent of whether it
    # ends up bound to a test. Anything left over after the #[test] walk
    # below never had a contiguous #[test] above it.
    all_covers_lines = {i + 1 for i, ln in enumerate(lines) if _COVERS_RE.match(ln)}
    consumed_covers_lines: set[int] = set()

    for i, line in enumerate(lines):
        if not _TEST_ATTR_RE.match(line):
            continue
        test_line_no = i + 1

        # Find the `fn` this #[test] attaches to, tolerating other
        # attributes (e.g. #[ignore]) in between but not a blank line.
        fn_name = None
        fn_line_no = None
        j = i + 1
        while j < len(lines):
            candidate = lines[j]
            m = _FN_RE.match(candidate)
            if m:
                fn_name = m.group(1)
                fn_line_no = j + 1
                break
            if candidate.strip() == "":
                break
            if _ATTR_RE.match(candidate):
                j += 1
                continue
            break

        # Walk upward from the #[test] line, collecting contiguous
        # `covers:` comments. Other attributes may sit between a covers:
        # comment and #[test]; a blank line breaks contiguity. This runs
        # regardless of whether a `fn` was found below: a covers: block is
        # still "contiguous with a #[test]" even when that #[test] itself
        # is malformed, which is a separate diagnostic below.
        covers_lines: list[tuple[int, str]] = []
        k = i - 1
        while k >= 0:
            stripped = lines[k].strip()
            if stripped == "":
                break
            cm = _COVERS_RE.match(lines[k])
            if cm:
                covers_lines.append((k + 1, cm.group(1)))
                k -= 1
                continue
            if _ATTR_RE.match(lines[k]):
                k -= 1
                continue
            break
        covers_lines.reverse()
        consumed_covers_lines.update(line_no for line_no, _rest in covers_lines)

        if fn_name is None:
            diagnostics.append(
                ContiguityDiagnostic(
                    kind="test_without_fn",
                    file=display,
                    line=test_line_no,
                    message=(
                        f"#[test] at {display}:{test_line_no} has no fn signature "
                        "after its attributes"
                    ),
                )
            )
            continue

        for line_no, rest in covers_lines:
            if ":" not in rest:
                continue
            card, _, branches_part = rest.partition(":")
            card = card.strip()
            branches = [b.strip() for b in branches_part.split(",") if b.strip()]
            for branch in branches:
                bindings.append(
                    Binding(
                        card=card,
                        branch=branch,
                        file=display,
                        line=fn_line_no,
                        test=fn_name,
                    )
                )

    for line_no in sorted(all_covers_lines - consumed_covers_lines):
        diagnostics.append(
            ContiguityDiagnostic(
                kind="orphaned_covers",
                file=display,
                line=line_no,
                message=(
                    f"covers: annotation at {display}:{line_no} is not contiguous "
                    "with a #[test]"
                ),
            )
        )

    return bindings, diagnostics


def scan_roots(roots: Iterable[Path]) -> tuple[list[Binding], list[ContiguityDiagnostic]]:
    bindings: list[Binding] = []
    diagnostics: list[ContiguityDiagnostic] = []
    for root in roots:
        if not root.exists():
            continue
        for rs_file in sorted(root.rglob("*.rs")):
            file_bindings, file_diagnostics = parse_file(rs_file)
            bindings.extend(file_bindings)
            diagnostics.extend(file_diagnostics)
    return bindings, diagnostics


def build_report(
    wave: str,
    manifest_path: Path,
    cards: dict[str, CardManifest],
    bindings: list[Binding],
    diagnostics: list[ContiguityDiagnostic] | None = None,
    declared_elsewhere: set[str] | None = None,
) -> dict[str, Any]:
    by_card_branch: dict[tuple[str, str], list[Binding]] = {}
    unknown_card_annotations: list[dict[str, Any]] = []
    undeclared_branch_annotations: list[dict[str, Any]] = []
    declared_elsewhere = declared_elsewhere or set()

    for b in bindings:
        card_manifest = cards.get(b.card)
        if card_manifest is None:
            # A card another wave's manifest declares is not a typo: the
            # scan roots hold every wave's annotations at once, so an
            # earlier wave's `covers:` lines are simply not this wave's
            # business. A name absent from *every* supplied manifest still
            # fails closed, which is what this check exists for.
            if b.card in declared_elsewhere:
                continue
            unknown_card_annotations.append(
                {"card": b.card, "branch": b.branch, "file": b.file, "line": b.line, "test": b.test}
            )
            continue
        if b.branch not in card_manifest.branches and b.branch not in card_manifest.unreachable:
            undeclared_branch_annotations.append(
                {"card": b.card, "branch": b.branch, "file": b.file, "line": b.line, "test": b.test}
            )
            continue
        by_card_branch.setdefault((b.card, b.branch), []).append(b)

    cards_report: dict[str, Any] = {}
    cards_covered = 0
    unexercised_total = 0
    for name in sorted(cards):
        card_manifest = cards[name]
        branches_report: dict[str, Any] = {}
        unexercised: list[str] = []
        for branch in card_manifest.branches:
            hits = by_card_branch.get((name, branch), [])
            branches_report[branch] = [
                {"file": h.file, "line": h.line, "test": h.test} for h in hits
            ]
            if not hits:
                unexercised.append(branch)
        unexercised_total += len(unexercised)
        if not unexercised:
            cards_covered += 1
        cards_report[name] = {
            "post_board": card_manifest.post_board,
            "branches": branches_report,
            "unexercised": unexercised,
            "unreachable": dict(card_manifest.unreachable),
        }

    # Contiguity diagnostics are additive information about *why* an
    # annotation never became a binding; they never gate the exit code by
    # themselves (a branch they explain is already counted, or not, by the
    # unexercised-branch check above -- exit code behavior is unchanged).
    ok = (
        unexercised_total == 0
        and not unknown_card_annotations
        and not undeclared_branch_annotations
    )
    return {
        "schema": REPORT_SCHEMA,
        "wave": wave,
        "manifest": _display_path(manifest_path),
        "cards": cards_report,
        "errors": {
            "unknown_card_annotations": unknown_card_annotations,
            "undeclared_branch_annotations": undeclared_branch_annotations,
        },
        "contiguity_diagnostics": [
            {"kind": d.kind, "file": d.file, "line": d.line, "message": d.message}
            for d in (diagnostics or [])
        ],
        "summary": {
            "cards_total": len(cards),
            "cards_covered": cards_covered,
            "unexercised_branches": unexercised_total,
        },
        "exit_code": 0 if ok else 1,
    }


def render_report(report: dict[str, Any]) -> str:
    lines: list[str] = []
    for name in sorted(report["cards"]):
        card = report["cards"][name]
        lines.append(f"== {name} (post_board: {str(card['post_board']).lower()}) ==")
        for branch, hits in card["branches"].items():
            if hits:
                where = ", ".join(f"{h['file']}:{h['line']} ({h['test']})" for h in hits)
                lines.append(f"  [OK] {branch}: {where}")
            else:
                lines.append(f"  [UNEXERCISED] {branch}")
        for branch, reason in card["unreachable"].items():
            lines.append(f"  [unreachable] {branch}: {reason}")

    errors = report["errors"]
    for entry in errors["unknown_card_annotations"]:
        lines.append(
            f"ERROR: unknown card {entry['card']!r} annotated at "
            f"{entry['file']}:{entry['line']} ({entry['test']}) is absent from the manifest"
        )
    for entry in errors["undeclared_branch_annotations"]:
        lines.append(
            f"ERROR: undeclared branch {entry['branch']!r} for card {entry['card']!r} at "
            f"{entry['file']}:{entry['line']} ({entry['test']}) is not declared in the manifest"
        )
    for diag in report["contiguity_diagnostics"]:
        lines.append(f"WARNING: {diag['message']}")

    summary = report["summary"]
    lines.append(
        "kernel-covered: "
        f"{summary['cards_covered']} of {summary['cards_total']} cards, "
        f"{summary['unexercised_branches']} unexercised branches"
    )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path, help="path to the wave branch manifest JSON")
    parser.add_argument(
        "--tests-root",
        dest="tests_roots",
        action="append",
        type=Path,
        default=None,
        help="directory to scan for *.rs covers: annotations "
        "(repeatable; default: mtg-kernel/tests and mtg-kernel/src)",
    )
    parser.add_argument("--json", dest="json_out", type=Path, default=None, help="also write the JSON report here")
    parser.add_argument(
        "--also-declared",
        dest="also_declared",
        action="append",
        type=Path,
        default=None,
        help="another wave's manifest whose cards are declared but not checked here "
        "(repeatable). Annotations naming those cards are skipped instead of failing as "
        "unknown; a name in no manifest at all still fails closed.",
    )
    args = parser.parse_args(argv)

    roots = args.tests_roots if args.tests_roots else [Path("mtg-kernel/tests"), Path("mtg-kernel/src")]

    try:
        wave, cards = load_manifest(args.manifest)
        declared_elsewhere: set[str] = set()
        for sibling in args.also_declared or []:
            _sibling_wave, sibling_cards = load_manifest(sibling)
            declared_elsewhere.update(sibling_cards)
    except ManifestError as exc:
        print(f"WAVE_BRANCH_COVERAGE: FAIL: {exc}", file=sys.stderr)
        return 1
    # A card this wave declares is always this wave's to check, even if a
    # sibling manifest also lists it.
    declared_elsewhere -= set(cards)

    bindings, diagnostics = scan_roots(roots)
    report = build_report(wave, args.manifest, cards, bindings, diagnostics, declared_elsewhere)
    print(render_report(report))

    if args.json_out is not None:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        # No sort_keys: `report` is already built in the documented order
        # (cards sorted by name, branches in manifest order) by
        # build_report; alphabetizing keys here would silently break that
        # contract for the JSON record while leaving the text output
        # (which does not go through json.dumps) correctly ordered.
        args.json_out.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")

    return report["exit_code"]


if __name__ == "__main__":
    raise SystemExit(main())
