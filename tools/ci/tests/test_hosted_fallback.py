"""The hosted fallback reruns only jobs that a self-hosted runner could not admit, once."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

HERE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HERE))
import hosted_fallback as fb  # noqa: E402

WORKFLOW = HERE.parents[1] / ".github" / "workflows" / "ci.yml"


def job(job_id, conclusion, labels, name="Rust tests (windows-2025, x)"):
    return {"id": job_id, "conclusion": conclusion, "labels": labels, "name": name, "runner_name": "ci-win-1"}


class FallbackTests(unittest.TestCase):
    def test_only_failed_self_hosted_jobs_with_the_marker_are_rerun(self):
        jobs = [job(1, "failure", ["self-hosted", "mtg-ci-windows"]),      # admission timeout
                job(2, "failure", ["self-hosted", "mtg-ci-windows"]),      # a real failure
                job(3, "failure", ["windows-2025"]),                       # hosted
                job(4, "success", ["self-hosted", "mtg-ci-windows"]),
                job(5, "failure", ["self-hosted", "mtg-ci-windows"])]      # log unreadable
        logs = {1: "hook ... " + fb.ADMISSION_MARKER + ": no CPU slots", 2: "test failed", 3: fb.ADMISSION_MARKER,
                4: fb.ADMISSION_MARKER, 5: None}
        assert [j["id"] for j in fb.admission_timeouts(jobs, logs.get)] == [1]

    def test_main_dispatches_one_hosted_rerun_or_nothing(self):
        calls = []

        def request(method, path, body=None):
            calls.append((method, path, body))
            if path.endswith("/jobs?per_page=100&page=1"):
                return json.dumps({"jobs": jobs}).encode()
            if path.endswith("/logs"):
                return (fb.ADMISSION_MARKER if path.split("/")[-2] == "1" else "ok").encode()
            return b""

        with tempfile.TemporaryDirectory() as directory:
            summary = Path(directory) / "summary.md"
            env = {"GITHUB_REPOSITORY": "o/r", "GITHUB_RUN_ID": "9", "FALLBACK_REF": "feature",
                   "GITHUB_STEP_SUMMARY": str(summary), "GH_TOKEN": "t"}
            for jobs, dispatched in (([job(1, "failure", ["self-hosted"])], True),
                                     ([job(2, "failure", ["self-hosted"])], False)):
                calls.clear()
                with patch.dict(os.environ, env), patch.object(fb, "_request", side_effect=request):
                    assert fb.main() == 0
                posts = [c for c in calls if c[0] == "POST"]
                if dispatched:
                    assert posts == [("POST", "/repos/o/r/actions/workflows/ci.yml/dispatches",
                                      {"ref": "feature", "inputs": {"windows_runner": '"windows-2025"'}})]
                else:
                    assert posts == []
            assert "dispatched ci.yml" in summary.read_text()

    def test_the_workflow_routes_by_the_input_and_never_falls_back_twice(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        assert "windows_runner:" in text
        # every place that chooses the self-hosted pool honors the dispatch input first
        assert text.count("vars.MTG_CI_WINDOWS_RUNNER") == text.count(
            "inputs.windows_runner || vars.MTG_CI_WINDOWS_RUNNER")
        block = text[text.index("  hosted-fallback:"):]
        assert "inputs.windows_runner == ''" in block.split("steps:")[0]
        assert "actions: write" in block.split("steps:")[0]
        for needed in ("rust-test-groups", "rust-limited-groups", "python-tests"):
            assert needed in block.split("steps:")[0]


if __name__ == "__main__":
    unittest.main()
