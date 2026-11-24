"""Exercise admission and owned-process cleanup without invoking Docker."""

import hashlib
import io
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

import hosted

HERE = Path(__file__).resolve().parent
PORTABLE_SCHEMA = "fleet-combined-trainer-build/v1"
VARIANT_SCHEMA = "fleet-combined-trainer-build-target-variant/v1"
BASH = shutil.which("bash")
BINS = ["native_expanded_training_run_v1", "checkpoint_container_v1"]
SPECS = ["example-1:feature_1,f2", "_plain", "2nd:f2"]
INVALID_SPECS = ("", "../escape", "-flag", "name:", "name:a,,b", "name:a,", "name:a/b",
                 "name:--all-features", "name:a;b", "name:$(id)", "name\n", "námé",
                 "name:a,b,a", "DONE", "SHA256SUMS", *BINS)


def bash(*args, cwd=HERE, target_cpu=None):
    env = {key: os.environ[key] for key in ("PATH", "SystemRoot", "TEMP", "TMP") if key in os.environ}
    if target_cpu is not None:
        env["PINNED_TARGET_CPU"] = target_cpu
    return subprocess.run([BASH, *args], cwd=cwd, env=env, text=True, capture_output=True, timeout=60)


def import_hosted(value):
    """Import hosted.py in a fresh interpreter with PINNED_TARGET_CPU set to `value` (None: unset)."""
    env = {key: item for key, item in os.environ.items() if key != "PINNED_TARGET_CPU"}
    if value is not None:
        env["PINNED_TARGET_CPU"] = value
    return subprocess.run([sys.executable, "-B", "-c", "import hosted; print(hosted.TARGET_CPU)"], cwd=HERE, env=env,
                          capture_output=True, text=True, timeout=60)


def build_sh(value):
    """Run build.sh as far as its first privilege check with PINNED_TARGET_CPU set to `value` (None: unset)."""
    return bash("build.sh", "0" * 40, "/work/out", target_cpu=value)


class TargetVariantTests(unittest.TestCase):
    def test_the_environment_selects_the_variant_and_an_unknown_name_is_refused(self):
        self.assertEqual(import_hosted(None).stdout.strip(), "default")
        for name in hosted.TARGET_CPUS:
            self.assertEqual(import_hosted(name).stdout.strip(), name)
        for name in ("", "bogus", "znver5 -C x", "ZNVER5"):
            result = import_hosted(name)
            self.assertNotEqual(result.returncode, 0, name)
            self.assertIn("unknown PINNED_TARGET_CPU", result.stderr)

    def test_the_workflow_names_the_repository_by_id_so_a_move_to_another_owner_keeps_the_job(self):
        workflow = (HERE.parents[1] / ".github/workflows/pinned-release-build.yml").read_text(encoding="utf-8")
        condition = re.search(r"\n  build:\n(?:    #.*\n)*    if: >-\n((?:      .*\n)+)", workflow).group(1)
        self.assertIn("github.repository_id == '1303347549' &&", condition)
        self.assertIn("github.event.pull_request.head.repo.full_name == github.repository", condition)
        # No owner or repository name: either would skip the job after a rename or a transfer.
        self.assertNotRegex(condition, r"github\.repository\s*==\s*'")
        self.assertNotIn("repository_owner", condition)

    def test_the_workflow_the_guard_and_the_build_helper_name_the_same_variants(self):
        workflow = (HERE.parents[1] / ".github/workflows/pinned-release-build.yml").read_text(encoding="utf-8")
        options = re.search(r"target_cpu:\n(?:.*\n)*?\s+options:\n((?:\s+- \S+\n)+)", workflow).group(1)
        helper = (HERE / "build.sh").read_text(encoding="utf-8")
        cases = re.search(r'case "\$variant" in\n((?:.*\n)*?)esac', helper).group(1)
        labels = re.findall(r"^\s+([a-z0-9-]+)\) rustflags=", cases, flags=re.M)
        self.assertEqual(tuple(re.findall(r"- (\S+)", options)), hosted.TARGET_CPUS)
        self.assertEqual(tuple(labels), hosted.TARGET_CPUS)
        self.assertEqual(hosted.TARGET_CPUS[0], "default")
        # The value reaches the shell as an environment variable, never as script text.
        for script in re.findall(r"run: \|\n((?:(?: {10}.*)?\n)+)", workflow):
            self.assertNotIn("inputs.target_cpu", script)

    @unittest.skipIf(not BASH or os.name == "nt" or (hasattr(os, "geteuid") and os.geteuid() == 0),
                     "runs build.sh up to its root check: needs bash and a user that is not root")
    def test_build_helper_gives_a_variant_another_manifest_schema_and_refuses_an_unknown_name(self):
        result = build_sh("bogus")
        self.assertEqual(result.returncode, 2)
        self.assertIn("Unknown PINNED_TARGET_CPU", result.stderr)
        for name in (None, "", "default"):
            result = build_sh(name)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(f"Build variant: default; manifest schema: {PORTABLE_SCHEMA}; target flags: none\n",
                          result.stderr)
        seen = set()
        for name in hosted.TARGET_CPUS[1:]:
            result = build_sh(name)
            self.assertNotEqual(result.returncode, 0)
            line = re.search(r"Build variant: (\S+); manifest schema: (\S+); target flags: (.+)\n", result.stderr)
            self.assertEqual(line.group(1, 2), (name, VARIANT_SCHEMA))
            self.assertIn("-C target-cpu=", line.group(3))
            seen.add(line.group(3))
        self.assertEqual(len(seen), len(hosted.TARGET_CPUS) - 1)
        self.assertNotEqual(PORTABLE_SCHEMA, VARIANT_SCHEMA)


class ExampleTests(unittest.TestCase):
    cli = ["--work-root", "root", "--source-commit", hosted.SOURCE]

    def test_default_dispatch_executes_the_original_guard_and_helper_bytes(self):
        guard = HERE / "bin-only/hosted.py"
        for extra in ([], ["--examples="]):
            with self.subTest(extra=extra), patch.object(hosted.os, "execv", side_effect=SystemExit) as launch:
                with self.assertRaises(SystemExit):
                    hosted.main(self.cli + extra)
                launch.assert_called_once_with(sys.executable, [sys.executable, "-B", str(guard), *self.cli,
                                                                "--wall-seconds", str(hosted.MAX_WALL)])
        self.assertEqual(hosted.sha256(guard), "40b712c9e376d9ced5743593b142cb40b0e727ad70f566ab50abc0c339bd7141")
        self.assertEqual(hosted.sha256(guard.with_name("build.sh")),
                         "ac283ee8d1b4f6725b16f3931128c7f61a83dbd9251bc924dbc1c5b494f51194")

    def test_main_passes_valid_specs_to_run_without_starting_a_shell(self):
        with (patch.object(hosted.sys, "platform", "linux"), patch.object(hosted.platform, "machine", return_value="x86_64"),
              patch.object(hosted.os, "geteuid", return_value=0, create=True), patch.object(hosted.os, "umask"),
              patch.object(hosted.signal, "signal"), patch.object(hosted, "create_root") as create,
              patch.object(subprocess, "Popen", side_effect=AssertionError("unexpected subprocess")),
              patch.object(sys, "stdout", new_callable=io.StringIO),
              patch.object(hosted, "run", return_value={"pass": True, "error": None}) as run):
            self.assertEqual(hosted.main([*self.cli, "--examples", " ".join(SPECS)]), 0)
            create.assert_called_once_with(Path("root"))
            run.assert_called_once_with(Path("root"), hosted.SOURCE, hosted.MAX_WALL, SPECS)

    def test_main_rejects_invalid_examples_before_admission(self):
        for raw in (" ", "a  b", " a", "a ", "a\tb", "a\nb", "name:a name:b", *INVALID_SPECS[1:]):
            with (self.subTest(raw=raw), patch.object(sys, "stderr", new_callable=io.StringIO) as stderr,
                  patch.object(subprocess, "Popen", side_effect=AssertionError("unexpected subprocess")),
                  patch.object(hosted, "create_root") as create, patch.object(hosted, "run") as run):
                with self.assertRaises(SystemExit) as caught:
                    hosted.main([*self.cli, "--examples=" + raw])
                self.assertEqual(caught.exception.code, 2)
                self.assertIn("example", stderr.getvalue())
                create.assert_not_called()
                run.assert_not_called()

    def test_run_rejects_invalid_direct_requests_before_admission(self):
        with patch.object(hosted, "Resources", side_effect=AssertionError("unexpected admission")):
            for request in (["bad name"], ["name:f", "name"], "plain"):
                with self.subTest(request=request), self.assertRaises(ValueError):
                    hosted.run(Path("root"), hosted.SOURCE, 1, request)

    @unittest.skipUnless(BASH, "requires Bash")
    def test_helper_validates_both_check_only_and_normal_invocations(self):
        self.assertEqual(bash("build.sh", "--check-examples", *SPECS).returncode, 0)
        for args in (["--check-examples"], [hosted.SOURCE, "/work/out"]):
            for specs in [(spec,) for spec in INVALID_SPECS] + [("name", "name:other")]:
                with self.subTest(args=args, specs=specs):
                    # A bad CPU stops even a regressed helper before machine setup.
                    # Let Bash pass literal newlines to the script, including on Windows.
                    command = shlex.join(["exec", "bash", "build.sh", *args, *specs])
                    result = bash("-c", command, target_cpu="invalid")
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertIn("example", result.stderr.lower())

    @unittest.skipUnless(BASH, "requires Bash")
    def test_build_helper_builds_installs_verifies_and_syncs_each_example(self):
        # Substitute only the machine-specific setup. Run argument parsing and the release steps as shipped.
        helper = (HERE / "build.sh").read_text(encoding="utf-8")
        setup = (HERE / "fixtures/example-build-setup.sh").read_text(encoding="utf-8")
        start = helper.index("[[ $EUID == 0")
        end = helper.index('mkdir -- "$work/home"', start)
        script = helper[:start] + setup + "\n" + helper[end:]
        names = ["example-1", "_plain", "2nd"]
        triple = "x86_64-unknown-linux-gnu"
        for cpu in ("default", "znver5"):
            with self.subTest(cpu=cpu), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "build.sh").write_text(script, encoding="utf-8", newline="\n")
                release = Path("release") if cpu == "default" else Path(triple, "release")
                sources = [release / name for name in BINS] + [release / "examples" / name for name in names]
                for src in sources:
                    path = root / "prebuilt" / src
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(src.name.encode())

                result = bash("build.sh", hosted.SOURCE, "/work/out", *SPECS, cwd=root, target_cpu=cpu)
                self.assertEqual(result.returncode, 0, result.stderr)

                def lines(file):
                    return (root / file).read_text(encoding="utf-8").splitlines()

                base = ["build", "--release", "--locked", "--jobs", "2", "-p", "mtg-kernel"]
                target = [] if cpu == "default" else ["--target", triple]
                calls = [["--bin", name] for name in BINS] + [
                    ["--example", "example-1", "--features", "feature_1,f2"],
                    ["--example", "_plain"], ["--example", "2nd", "--features", "f2"]]
                self.assertEqual(lines("target/cargo-args"), ["\t".join(base + call + target) for call in calls])
                self.assertEqual(lines("target/rustflags"), ["" if cpu == "default" else "-C target-cpu=znver5"] * len(calls))
                self.assertEqual(lines("comparisons"), [f"{src.as_posix()}\t{src.name}" for src in sources])
                self.assertEqual(lines("manifest-args")[-(1 + len(BINS) + len(SPECS)):], [*BINS, "--", *SPECS])
                files = [*BINS, *names, "BUILD-MANIFEST.json", "rust-components.json", "debian-packages.tsv"]
                for src in sources:
                    self.assertEqual((root / "out" / src.name).read_bytes(), src.name.encode())
                # GNU sha256sum may mark binary inputs with '*' on Windows.
                sums = [line.replace(" *", "  ", 1) for line in lines("out/SHA256SUMS")]
                self.assertEqual(sums, [f"{hashlib.sha256((root / 'out' / file).read_bytes()).hexdigest()}  {file}"
                                        for file in files])
                self.assertIn("--check SHA256SUMS", lines("checksum-commands"))
                self.assertIn(" ".join(["--", *files, "SHA256SUMS", "build.log"]), lines("sync-commands"))
                self.assertTrue((root / "out/DONE").is_file())

    def test_manifest_records_installed_examples_using_the_binary_metadata(self):
        # Execute the manifest writer against fixture files. Only its external Linux tool queries are mocked.
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            bins = ["native_expanded_training_run_v1", "checkpoint_container_v1"]
            specs = ["cube_b_census_v1:eternal-fixtures,other_2", "plain"]
            for name in [*bins, "Cargo.lock", "rust-components.json", "debian-packages.tsv", "cube_b_census_v1", "plain"]:
                (root / name).write_bytes(name.encode())

            def command(args, **kwargs):
                if args[:2] == ("gcc", "-print-prog-name=ld"):
                    return sys.executable
                if args[0] == "readelf":
                    return "Advanced Micro Devices X86-64 /lib64/ld-linux-x86-64.so.2"
                return "fixture"

            def write(script, arguments):
                source = script.read_text(encoding="utf-8").split("<<'PY'\n")[-1].split("\nPY\n")[0]
                argv = ["-", str(root), str(root), hosted.SOURCE, "origin", "123", *bins, *arguments]
                with patch.object(sys, "argv", argv), patch.object(subprocess, "check_output", side_effect=command):
                    exec(compile(source, str(script), "exec"), {})
                return (root / "BUILD-MANIFEST.json").read_bytes()

            for cpu, flags, schema in (("default", "", PORTABLE_SCHEMA), ("znver5", "-C target-cpu=znver5", VARIANT_SCHEMA)):
                with patch.dict(os.environ, PINNED_VARIANT=cpu, PINNED_RUSTFLAGS=flags, PINNED_MANIFEST_SCHEMA=schema):
                    original = write(HERE / "bin-only/build.sh", [])
                    self.assertEqual(write(HERE / "build.sh", ["--"]), original)
                    manifest = json.loads(write(HERE / "build.sh", ["--", *specs]))
                    examples = manifest.pop("examples")
                    self.assertEqual(manifest["build"].pop("examples"), specs)
                    self.assertEqual(manifest, json.loads(original))
                    for entry, name, features in zip(examples, ["cube_b_census_v1", "plain"],
                                                     [["eternal-fixtures", "other_2"], []], strict=True):
                        self.assertEqual(entry, dict(name=name, bytes=len(name), features=features,
                                                     runtime_libraries=["fixture"],
                                                     sha256=hashlib.sha256(name.encode()).hexdigest()))


class HostedGuardTests(unittest.TestCase):
    def setUp(self):
        self.parent = Path(self.enterContext(tempfile.TemporaryDirectory())).resolve()
        self.root = self.parent / "work-root"
        hosted.create_root(self.root)
        self.enterContext(patch.object(hosted, "mem_available", return_value=14 * hosted.GIB))
        self.original_swap = {"type": "partition", "size_bytes": 8 * hosted.GIB, "used_bytes": 0, "priority": 5}
        self.swaps = {"/dev/previous": self.original_swap}
        self.enterContext(patch.object(hosted, "active_swaps", side_effect=lambda: dict(self.swaps)))

    def test_existing_root_is_refused(self):
        with self.assertRaises(FileExistsError):
            hosted.create_root(self.root)

    def test_capacity_refusal_records_resources_before_starting_docker(self):
        free = hosted.FREE_FLOOR + hosted.TOTAL_GROWTH - 1
        with (patch.object(hosted.shutil, "disk_usage", return_value=SimpleNamespace(free=free)),
              patch.object(hosted, "allocated_bytes", return_value=0), patch.object(hosted, "start") as start,
              patch.object(hosted, "swap_command") as swap):
            receipt = hosted.run(self.root, hosted.SOURCE, 1)
        start.assert_not_called()
        swap.assert_not_called()
        self.assertFalse(receipt["pass"])
        self.assertIn("81.5 GiB", receipt["error"])
        self.assertEqual(receipt["resources"]["initial_free_bytes"], free)
        self.assertEqual(receipt["swap"]["original"], self.swaps)
        self.assertEqual(json.loads((self.root / "guard.json").read_bytes()), receipt)

    def test_the_variant_reaches_the_container_and_the_receipt(self):
        with patch.object(hosted, "TARGET_CPU", "znver5"):
            self.exercise_cleanup(growth_stop=True, target_cpu="znver5")

    def test_examples_reach_the_container_as_separate_arguments(self):
        _, receipt = self.exercise_cleanup(examples=SPECS)
        self.assertEqual(receipt["examples"], SPECS)
        self.assertEqual(json.loads((self.root / "guard.json").read_bytes())["examples"], SPECS)

    def exercise_cleanup(self, growth_stop=False, swapoff_error=False, target_cpu="default", examples=()):
        free = 85 * hosted.GIB
        daemon = Mock(pid=101, returncode=None)
        daemon.poll.return_value = None
        builder = Mock(pid=102, returncode=None if growth_stop else 101)
        builder.poll.return_value = builder.returncode
        daemon.wait.side_effect = lambda **_: setattr(daemon.poll, "return_value", 0)
        events = []
        commands = []
        running = growth_stop

        def start(command, log, env):
            commands.append((command, env))
            log.write_text("retained log\n", encoding="ascii")
            return daemon if len(commands) == 1 else builder

        def swap(command, path, _log):
            self.assertEqual(path, self.root / "swapfile")
            events.append(command[0])
            if command[0] == "fallocate":
                path.write_bytes(b"x" * hosted.SWAP)
            elif command[0] == "swapon":
                self.swaps[str(path)] = {"type": "file", "size_bytes": hosted.SWAP, "used_bytes": 0, "priority": -2}
            elif command[0] == "swapoff":
                if swapoff_error:
                    raise OSError("swapoff failed")
                del self.swaps[str(path)]

        def docker(command, **_):
            nonlocal running
            self.assertEqual(command[:5], ["/usr/bin/docker", "--config", str(self.root / "client"), "--host",
                                            f"unix://{self.root / 'docker.sock'}"])
            self.assertEqual(command[-1], hosted.CONTAINER)
            action = command[6]
            events.append(action)
            if action == "stop":
                running = False
            state = {"Running": running, "OOMKilled": not growth_stop, "ExitCode": 0 if running else 101}
            return subprocess.CompletedProcess(command, 0, json.dumps(state).encode(), b"")

        with (patch.object(hosted.shutil, "disk_usage", return_value=SimpleNamespace(free=free)),
              patch.object(hosted, "SWAP", 16), patch.object(hosted, "TOTAL_GROWTH", hosted.GROWTH + 16),
              patch.object(hosted, "file_blocks", side_effect=lambda path: path.stat().st_size if path.exists() else 0),
              patch.object(hosted, "allocated_bytes", side_effect=[0, 0, 16, 16, 16,
                                                                   hosted.GROWTH + 17 if growth_stop else 16]),
              patch.object(Path, "is_socket", return_value=True),
              patch.object(hosted, "executable", side_effect=lambda name: f"/usr/bin/{name}"),
              patch.object(hosted, "start", side_effect=start), patch.object(hosted, "swap_command", side_effect=swap),
              patch.object(hosted.subprocess, "run", side_effect=docker),
              patch.object(hosted.signal, "SIGKILL", 9, create=True),
              patch.object(hosted.os, "killpg", side_effect=lambda pid, sig: events.append((pid, sig)), create=True)):
            receipt = hosted.run(self.root, hosted.SOURCE, 60, examples)

        self.assertEqual(commands[0][1]["DOCKER_TMPDIR"], str(self.root / "tmp"))
        self.assertEqual(commands[0][1]["TMPDIR"], str(self.root / "tmp"))
        self.assertIn("--bridge=none", commands[0][0])
        self.assertIn("--iptables=false", commands[0][0])
        self.assertIn(f"type=bind,src={self.root / 'work'},dst=/work", commands[1][0])
        self.assertIn("--memory-swap=28g", commands[1][0])
        self.assertNotIn("--rm", commands[1][0])
        self.assertEqual(commands[1][0][-(4 + len(examples)):],
                         [hosted.IMAGE, "/build.sh", hosted.SOURCE, "/work/out", *examples])
        # The target-flag variant reaches build.sh through the container's environment; the default is `default`.
        position = commands[1][0].index("PINNED_TARGET_CPU=" + target_cpu)
        self.assertEqual(commands[1][0][position - 1], "--env")
        self.assertEqual([item for item in commands[1][0] if item.startswith("PINNED_TARGET_CPU=")],
                         ["PINNED_TARGET_CPU=" + target_cpu])
        self.assertEqual(receipt["target_cpu"], target_cpu)
        self.assertFalse(receipt["pass"])
        self.assertTrue((self.root / "daemon.log").is_file())
        self.assertTrue((self.root / "container.log").is_file())
        self.assertTrue((self.root / "swapfile").is_file())
        self.assertEqual(receipt["swap"]["original"], {"/dev/previous": self.original_swap})
        self.assertEqual(receipt["swap"]["after_cleanup"]["/dev/previous"], self.original_swap)
        return events, receipt

    def test_owned_growth_stop_keeps_logs_and_disables_only_owned_swap_after_docker(self):
        events, receipt = self.exercise_cleanup(growth_stop=True)
        self.assertEqual(events, ["fallocate", "mkswap", "swapon", (102, signal.SIGTERM), "inspect", "stop", "inspect",
                                  "rm", (101, signal.SIGTERM), "swapoff"])
        self.assertIn("growth exceeded", receipt["error"])
        self.assertEqual(receipt["resources"]["maximum_owned_build_allocated_bytes"], hosted.GROWTH + 1)
        self.assertNotIn(str(self.root / "swapfile"), receipt["swap"]["after_cleanup"])

    def test_compiler_exit_captures_oom_before_removal_and_records_swapoff_failure(self):
        events, receipt = self.exercise_cleanup(swapoff_error=True)
        self.assertEqual(events, ["fallocate", "mkswap", "swapon", "inspect", "rm", (101, signal.SIGTERM), "swapoff"])
        self.assertTrue(receipt["container"]["before_cleanup"]["OOMKilled"])
        self.assertEqual(receipt["build_exit_code"], 101)
        self.assertIn("owned swapoff: swapoff failed", receipt["cleanup_errors"])
        self.assertIn(str(self.root / "swapfile"), receipt["swap"]["after_cleanup"])

    def test_container_that_was_never_created_needs_no_stop_or_removal(self):
        docker = ["/usr/bin/docker", "--host", f"unix://{self.root / 'docker.sock'}"]
        log = self.root / "container.log"
        diagnostic = f"Error response from daemon: No such container: {hosted.CONTAINER}".encode()
        receipt = {}
        with patch.object(hosted.subprocess, "run",
                          return_value=subprocess.CompletedProcess(docker, 1, b"", diagnostic)) as invoke:
            hosted.remove_container(docker, {}, log, receipt)
        invoke.assert_called_once()
        self.assertEqual(receipt, {"before_cleanup": None})
        self.assertEqual(log.read_bytes(), diagnostic)
