"""Windows workflow jobs route TEMP and TMP into runner.temp so per-job cleanup removes leftovers."""
import re
import unittest
from pathlib import Path

WORKFLOWS_DIR = Path(__file__).resolve().parents[3] / ".github" / "workflows"
JOB_HEADER_RE = re.compile(r"^  ([a-zA-Z0-9_-]+):\s*$")


def iter_workflow_jobs(workflow_path: Path):
    lines = workflow_path.read_text(encoding="utf-8").splitlines()
    in_jobs = False
    current_job = None
    current_lines = []
    for line in lines:
        if re.match(r"^jobs:\s*$", line):
            in_jobs = True
            continue
        if not in_jobs:
            continue
        if line and not line.startswith(" ") and not line.startswith("#"):
            break
        match = JOB_HEADER_RE.match(line)
        if match:
            if current_job is not None:
                yield current_job, "\n".join(current_lines)
            current_job = match.group(1)
            current_lines = [line]
        elif current_job is not None:
            current_lines.append(line)
    if current_job is not None:
        yield current_job, "\n".join(current_lines)


def job_can_run_on_windows(job_block: str) -> bool:
    header = job_block.split("\n    steps:\n", 1)[0]
    non_comment = [
        line for line in header.splitlines() if not line.lstrip().startswith("#")
    ]
    runs_on = next(
        (line.split("runs-on:", 1)[1].strip() for line in non_comment if line.startswith("    runs-on:")),
        "",
    )
    if "windows" in runs_on.lower():
        return True
    if "matrix." in runs_on:
        return "windows" in "\n".join(non_comment).lower()
    return False


class WindowsRunnerTempTests(unittest.TestCase):
    def test_windows_jobs_point_temp_and_tmp_at_runner_temp_before_checkout(self):
        discovered_windows_jobs = set()
        workflow_files = sorted(WORKFLOWS_DIR.glob("*.yml"))
        self.assertGreaterEqual(len(workflow_files), 3)

        for workflow in workflow_files:
            for job_name, job_block in iter_workflow_jobs(workflow):
                header, steps = job_block.split("\n    steps:\n", 1)
                non_comment_header = "\n".join(
                    line for line in header.splitlines() if not line.lstrip().startswith("#")
                )
                self.assertNotIn(
                    "runner.temp",
                    non_comment_header,
                    f"{workflow.name}:{job_name} must not reference runner.temp in job-level headers/env",
                )
                if not job_can_run_on_windows(job_block):
                    continue
                discovered_windows_jobs.add((workflow.name, job_name))
                self.assertIn(
                    "- name: Point Windows TEMP and TMP at runner.temp",
                    steps,
                    f"{workflow.name}:{job_name} must point Windows TEMP and TMP at runner.temp",
                )
                temp_step_pos = steps.index("- name: Point Windows TEMP and TMP at runner.temp")
                checkout_pos = steps.index("uses: actions/checkout@")
                self.assertLess(
                    temp_step_pos,
                    checkout_pos,
                    f"{workflow.name}:{job_name} must set TEMP/TMP before checkout and test steps",
                )
                step_slice = steps[temp_step_pos:checkout_pos]
                self.assertIn("if: runner.os == 'Windows'", step_slice)
                self.assertIn("shell: bash", step_slice)
                self.assertIn("WINDOWS_RUNNER_TEMP: ${{ runner.temp }}", step_slice)
                self.assertIn('runner_temp="$(cygpath -w "$WINDOWS_RUNNER_TEMP")"', step_slice)
                self.assertIn(
                    'printf \'TEMP=%s\\nTMP=%s\\n\' "$runner_temp" "$runner_temp" >> "$GITHUB_ENV"',
                    step_slice,
                )

        expected = {
            ("ci.yml", "rust-test-groups"),
            ("ci.yml", "limited-build"),
            ("ci.yml", "rust-limited-groups"),
            ("ci.yml", "python-env-build"),
            ("ci.yml", "python-tests"),
            ("s-transformer-v1.yml", "receipts"),
            ("w5-tools.yml", "unit"),
        }
        self.assertTrue(expected.issubset(discovered_windows_jobs))


if __name__ == "__main__":
    unittest.main()
