from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
TOOLS = REPO_ROOT / "python" / "tools"
if str(TOOLS) not in sys.path:
    sys.path.insert(0, str(TOOLS))

import check_wave_branch_coverage_v1 as checker  # noqa: E402


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _run(manifest: Path, tests_root: Path, json_out: Path | None = None) -> tuple[int, str]:
    args = [str(manifest), "--tests-root", str(tests_root)]
    if json_out is not None:
        args += ["--json", str(json_out)]
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        code = checker.main(args)
    return code, buf.getvalue()


class CheckWaveBranchCoverageTest(unittest.TestCase):
    def test_one_covered_card_one_card_with_an_unexercised_branch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            _write(
                manifest_path,
                json.dumps(
                    {
                        "schema": "kernel_wave_branch_manifest/v1",
                        "wave": "synthetic_w1",
                        "cards": {
                            "Fully Covered Card": {
                                "post_board": False,
                                "branches": ["branch_a", "branch_b"],
                            },
                            "Partly Covered Card": {
                                "post_board": True,
                                "branches": ["branch_x", "branch_y"],
                            },
                        },
                    }
                ),
            )
            tests_root = root / "tests"
            _write(
                tests_root / "fake_wave_tests.rs",
                "\n".join(
                    [
                        "// covers: Fully Covered Card: branch_a, branch_b",
                        "#[test]",
                        "fn fully_covered_card_does_both_things() {",
                        "    assert!(true);",
                        "}",
                        "",
                        "// covers: Partly Covered Card: branch_x",
                        "#[test]",
                        "fn partly_covered_card_does_one_thing() {",
                        "    assert!(true);",
                        "}",
                        "",
                    ]
                ),
            )

            code, out = _run(manifest_path, tests_root)

            self.assertEqual(code, 1, out)
            self.assertIn("branch_y", out, "the unexercised branch must be named in the output")
            self.assertIn("UNEXERCISED", out)
            self.assertIn(
                "fully_covered_card_does_both_things",
                out,
                "the covered card must be reported with its covering test name",
            )
            self.assertIn("kernel-covered: 1 of 2 cards", out)
            self.assertIn("1 unexercised branches", out)

    def test_annotation_naming_an_undeclared_branch_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            _write(
                manifest_path,
                json.dumps(
                    {
                        "schema": "kernel_wave_branch_manifest/v1",
                        "wave": "synthetic_w1",
                        "cards": {
                            "Typo Card": {
                                "post_board": False,
                                "branches": ["real_branch"],
                            }
                        },
                    }
                ),
            )
            tests_root = root / "tests"
            _write(
                tests_root / "fake_typo_tests.rs",
                "\n".join(
                    [
                        "// covers: Typo Card: real_branch",
                        "// covers: Typo Card: reel_branch",
                        "#[test]",
                        "fn typo_card_does_the_thing() {",
                        "    assert!(true);",
                        "}",
                        "",
                    ]
                ),
            )

            code, out = _run(manifest_path, tests_root)

            self.assertEqual(code, 1, out)
            self.assertIn("undeclared branch", out.lower())
            self.assertIn("reel_branch", out)

    def test_fully_covered_manifest_exits_zero_and_writes_json_report(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            _write(
                manifest_path,
                json.dumps(
                    {
                        "schema": "kernel_wave_branch_manifest/v1",
                        "wave": "synthetic_w1",
                        "cards": {
                            "Solo Card": {
                                "post_board": False,
                                "branches": ["only_branch"],
                                "unreachable": {"phantom_branch": "not reachable in this pool"},
                            }
                        },
                    }
                ),
            )
            tests_root = root / "tests"
            _write(
                tests_root / "fake_solo_tests.rs",
                "\n".join(
                    [
                        "// covers: Solo Card: only_branch",
                        "#[test]",
                        "fn solo_card_does_the_thing() {",
                        "    assert!(true);",
                        "}",
                        "",
                    ]
                ),
            )
            json_out = root / "report.json"

            code, out = _run(manifest_path, tests_root, json_out=json_out)

            self.assertEqual(code, 0, out)
            self.assertIn("kernel-covered: 1 of 1 cards", out)
            self.assertIn("0 unexercised branches", out)
            self.assertIn("phantom_branch", out, "unreachable branches are still printed")

            report = json.loads(json_out.read_text(encoding="utf-8"))
            self.assertEqual(report["schema"], "kernel_wave_branch_coverage_report/v1")
            self.assertEqual(report["exit_code"], 0)
            self.assertEqual(
                report["cards"]["Solo Card"]["unreachable"],
                {"phantom_branch": "not reachable in this pool"},
            )

    def test_stacked_covers_lines_and_ignore_attribute_are_contiguous(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            _write(
                manifest_path,
                json.dumps(
                    {
                        "schema": "kernel_wave_branch_manifest/v1",
                        "wave": "synthetic_w1",
                        "cards": {
                            "Stacked Card": {
                                "post_board": False,
                                "branches": ["first_branch"],
                            },
                            "Second Stacked Card": {
                                "post_board": False,
                                "branches": ["second_branch"],
                            },
                        },
                    }
                ),
            )
            tests_root = root / "tests"
            _write(
                tests_root / "fake_stacked_tests.rs",
                "\n".join(
                    [
                        "// covers: Stacked Card: first_branch",
                        "// covers: Second Stacked Card: second_branch",
                        "#[ignore]",
                        "#[test]",
                        "fn stacked_card_does_two_things() {",
                        "    assert!(true);",
                        "}",
                        "",
                    ]
                ),
            )

            code, out = _run(manifest_path, tests_root)

            self.assertEqual(code, 0, out)
            self.assertIn("stacked_card_does_two_things", out)

    def test_unknown_card_annotation_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest_path = root / "manifest.json"
            _write(
                manifest_path,
                json.dumps(
                    {
                        "schema": "kernel_wave_branch_manifest/v1",
                        "wave": "synthetic_w1",
                        "cards": {
                            "Known Card": {
                                "post_board": False,
                                "branches": ["known_branch"],
                            }
                        },
                    }
                ),
            )
            tests_root = root / "tests"
            _write(
                tests_root / "fake_unknown_tests.rs",
                "\n".join(
                    [
                        "// covers: Known Card: known_branch",
                        "#[test]",
                        "fn known_card_does_the_thing() {",
                        "    assert!(true);",
                        "}",
                        "",
                        "// covers: Nonexistent Card: some_branch",
                        "#[test]",
                        "fn nonexistent_card_test() {",
                        "    assert!(true);",
                        "}",
                        "",
                    ]
                ),
            )

            code, out = _run(manifest_path, tests_root)

            self.assertEqual(code, 1, out)
            self.assertIn("Nonexistent Card", out)


if __name__ == "__main__":
    unittest.main()
