"""Which checks a CI run needs: the one table behind the workflow's scoped checks and docs/CI-SCOPES.md.

A ready pull request whose files all map to one scope runs only that scope's checks; files of more than one
scope, or any file no rule names, run the whole pull request set. Docs files never decide. The full matrix
still runs on every merge (main, tags, release lines) as a monitor, so a scope only decides what gates a merge
(collab HANDOFFS/20261117/COORD.md, "Standing rule, merges (2026-11-21 07:27 ET)").

Usage:
  ci_scope.py choose          detect-changes step: reads EVENT, SCOPE (the group set chosen before), INPUT_SCOPE,
                              LABELS (JSON list), CHANGED_FILES (newline list); writes checks=<name> to GITHUB_OUTPUT
                              and a summary (with the bit-identity rows for trainer) to GITHUB_STEP_SUMMARY.
  ci_scope.py commands SCOPE  print the shell commands a scope's local run executes, one per line.
  ci_scope.py trainer-filters print the library test filters of the trainer scope, space separated.
  ci_scope.py markdown        print docs/CI-SCOPES.md.
"""
from __future__ import annotations

import fnmatch
import json
import os
import sys

# Each rule: (patterns, scope). The first rule naming a file decides it. "shared" files are read by both the
# engine (or agents, search, ladder) and the trainer: a change there is mixed and runs the whole pull request
# set, never a trainer-only check.
PATH_RULES: list[tuple[tuple[str, ...], str]] = [
    (("data/*",), "pr"),
    (("docs/*", "*.md"), "docs"),
    ((".github/workflows/ci.yml", ".github/actions/*", "tools/ci/*"), "pr"),
    (("tools/w5/*", "tools/ex1/target_ref.py", "tools/spy_reset/panel.py", "tools/spy_reset/pilot.py",
      "python/tools/nine_deck_public_panel_v1.py", "python/tools/s_transformer_reference_v1.py",
      ".github/workflows/w5-tools.yml"), "w5-tools"),
    ((".github/workflows/*",), "workflow"),
    (("tools/*",), "tools"),
    # Observation encoding, action windows, mulligans, sampling and the shared policy network.
    (("mtg-kernel/src/expanded_deck_training_v1/public_features.rs",
      "mtg-kernel/src/expanded_deck_training_v1/public_features/*",
      "mtg-kernel/src/expanded_deck_training_v1/stack_features.rs",
      "mtg-kernel/src/expanded_deck_training_v1/stack_features/*",
      "mtg-kernel/src/*mulligan*", "mtg-kernel/src/*/*mulligan*",
      "mtg-kernel/src/*action_windows*", "mtg-kernel/src/*/*action_windows*",
      "mtg-kernel/src/policy_observation_*", "mtg-kernel/src/native_flat_tensorizer_*",
      "mtg-kernel/src/native_policy_value_net_v1*", "mtg-kernel/src/fast_sampler.rs",
      "mtg-kernel/src/unclamped_softmax_sampler_v1.rs", "mtg-kernel/src/sideboard_play_policy_v1*",
      "mtg-kernel/src/rl_session.rs", "mtg-kernel/src/rl_session/*"), "shared"),
    (("mtg-kernel/src/expanded_deck_training_v1.rs", "mtg-kernel/src/expanded_deck_training_v1/*",
      "mtg-kernel/src/native_policy_train_step_v1.rs", "mtg-kernel/src/native_policy_train_step_v1/*",
      "mtg-kernel/src/native_expanded_training_run_v1.rs", "mtg-kernel/src/native_expanded_training_run_v1/*",
      "mtg-kernel/src/bin/native_expanded_training_run_v1.rs", "mtg-kernel/src/bin/expanded_deck_training_v1.rs",
      "mtg-kernel/src/bin/checkpoint_container_v1.rs"), "trainer"),
    (("mtg-kernel/src/bin/kernel_limited_*", "python/tools/check_limited_*", "python/tools/limited_match_v1.py"),
     "rules-limited"),
    # Python files that change which Rust tests run stay with the whole set.
    (("python/tools/run_rust_workspace_tests.py", "python/tools/run_rust_lib_filters.py",
      "python/tools/rust_case_seconds.json", "python/tools/rust_case_timeout_seconds.json",
      "python/tools/rust_limited_only_known_failures.json", "python/tools/run_rust_integration_tests.py",
      "python/tools/rust_integration_module_seconds.json", "python/tools/check_rust_integration_inventory.py",
      "python/tools/limited_*_session_v*.py"), "pr"),
    (("python/tests/*", "python/tools/*"), "python"),
]
NEUTRAL = {"docs"}
# What each scope runs. `jobs` are ci.yml job ids; the whole-set jobs gate themselves by scope as before.
SCOPES: dict[str, dict] = {
    "pr": {"jobs": ["the whole pull request set"],
           "summary": "every job of the pull request set (today's behavior)"},
    "trainer": {"jobs": ["rust-quality", "trainer-tests"],
                "summary": "formatting and Clippy, plus the library tests of the trainer and learner modules and "
                           "of what feeds a training update (TRAINER_FILTERS), Linux"},
    "tools": {"jobs": ["python-tools-tests"],
              "summary": "the top-level tools, G(i) and lease-guard unit suites (no Rust binary), Linux"},
    "w5-tools": {"jobs": [],
                 "summary": "nothing in ci.yml: w5-tools.yml runs its Linux (Python 3.11) and Windows unit jobs and "
                            "the cgroup acceptance on these paths"},
    "python": {"jobs": ["python-env-build", "python-tests"],
               "summary": "the Python test binaries and shards, Linux and Windows"},
    "rules-limited": {"jobs": ["rust-quality", "limited-build", "rust-limited-groups"],
                      "summary": "formatting and Clippy, the shared Limited build and every Limited group, Linux"},
    "workflow": {"jobs": [], "summary": "nothing in ci.yml: the changed workflow runs itself on its own paths"},
    "docs": {"jobs": [], "summary": "nothing"},
}
SCOPED = [name for name in SCOPES if name != "pr"]
DISPATCH_SCOPES = ["trainer", "tools", "python", "rules-limited"]
# Library test filters (libtest substring filters) of the trainer scope: the trainer and learner modules and
# everything that feeds a training update (GAE and the policy train step, both backward paths and their
# differential tests, the tensorizers and replay caches, the keep-lean and pass-lean offsets, which live in
# expanded_deck_training_v1, the checkpoint container and receipt code).
TRAINER_FILTERS = [
    "expanded_deck_training_v1::", "native_policy_train_step_v1::", "native_expanded_training_run_v1::",
    "native_policy_value_net_v1::", "training_math_v1::", "native_flat_tensorizer_v", "inference_cache_v1::",
    "native_train_state_payload_v1::", "native_trainer_schedule_v", "native_trainer_v1::", "native_training_",
    "native_checkpoint_", "native_baseline_checkpoint_chain_v4::", "fast_sampler::", "unclamped_softmax_sampler_v1::",
]
# Trainer changes also need these rows, which run on a node, never in CI.
BIT_IDENTITY_ROWS = [
    "the 10-game main-chain recipe", "the 60-game recipe", "the pilot recipe with mulligans",
    "the pilot recipe with mulligans and priority windows",
]
BIT_IDENTITY_BUILDS = "on the znver5 build and on the portable build, and on native arm64 when Mac arms use that build"
TRAINER_RULE = ("A trainer pull request without those rows linked is not mergeable under the merge rule, "
                "whatever CI says.")
RULES_NOTE = ("Engine rules and card changes that alter play are never bit-identical by scope: they run the rules "
              "groups (the whole set, or rules-limited), and, if a running lineage's trainer would pick them up, "
              "need a declaration.")
# Shell commands of each scope's local run (tools/ci/run_scope_local.py), from the repository root. The tools
# commands are the same lines the python-tests job runs (a test keeps them equal).
TOOLS_COMMANDS = [
    "uv run --no-sync python -m unittest discover -v -s python/tools/phase1_cloud -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/search_sharing_allocation -p 'test_cloud_lease.py'",
    "cd python/tools/tests && PYTHONPATH=.. uv run --no-sync python -m unittest -v test_nine_deck_cloud_v1.PackageTests.test_tooling_closure_is_complete_from_a_copy",
    "cd python/tools/tests && PYTHONPATH=.. uv run --no-sync python -m unittest -v test_g_references_campaign_v1 test_g_references_qualify_v1 test_g_references_cloud_v1 test_g_references_archive_v1",
    "uv run --no-sync python -m unittest discover -v -s tools/search_experiment/tests -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/stage4a/tests -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/t1_diagnosis -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/ci/tests -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/w5/tests -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/sd1/tests -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/native_fixture_bench/tests -p 'test_*.py'",
    "uv run --no-sync python -m unittest discover -v -s tools/checkpoint_codec_bench/tests -p 'test_*.py'",
]


def scope_of(path: str) -> str | None:
    for patterns, scope in PATH_RULES:
        if any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns):
            return scope
    return None


def scope_of_files(paths: list[str]) -> str:
    """One scope for a change: docs files never decide; one remaining scope wins; none means docs; anything
    else (mixed, shared or unmapped) is the whole pull request set."""
    found = set()
    for path in paths:
        scope = scope_of(path)
        if scope not in NEUTRAL:
            found.add(scope)
    if not found:
        return "docs"
    if len(found) == 1:
        (only,) = found
        if only in SCOPES and only != "pr":
            return only
    return "pr"


def choose(event: str, group_set: str, input_scope: str, labels: list[str], files: list[str]) -> str:
    """checks=all keeps the run's group set as chosen before (full, pr, draft, windows-*); a scope name runs
    only that scope's jobs."""
    if event == "workflow_dispatch":
        return input_scope if input_scope in DISPATCH_SCOPES else "all"
    if event != "pull_request" or group_set != "pr":
        return "all"
    if "ci:full" in labels:
        return "all"
    forced = [label[3:] for label in labels if label.startswith("ci:") and label[3:] in SCOPES]
    if len(forced) == 1:
        return "all" if forced[0] == "pr" else forced[0]
    scope = scope_of_files(files)
    return "all" if scope == "pr" else scope


def trainer_record() -> str:
    rows = "; ".join(BIT_IDENTITY_ROWS)
    return (f"Trainer scope: CI runs formatting, Clippy and the trainer library tests only. The bit-identity rows "
            f"go with every trainer change and run on a node, not in CI: {rows}, {BIT_IDENTITY_BUILDS}. "
            f"{TRAINER_RULE}")


def markdown() -> str:
    out = ["# CI scopes", "",
           "Generated by `python tools/ci/ci_scope.py markdown` from `tools/ci/ci_scope.py`; "
           "`tools/ci/tests/test_ci_scope.py` fails if this file and the table differ.", "",
           "A ready pull request whose files all map to one scope runs only that scope's checks. Files of more than "
           "one scope, a shared file, or a file no rule names run the whole pull request set. Docs files never "
           "decide. Main pushes, tags and dispatches without `scope` run the full matrix, which also monitors every "
           "merge. `ci:full` on a pull request forces the whole pull request set; `ci:<scope>` forces one scope; "
           "`gh workflow run ci.yml --ref REF -f scope=SCOPE` runs one scope on any ref.", "",
           "## Path rules (first match decides)", "", "| paths | scope |", "|---|---|"]
    for patterns, scope in PATH_RULES:
        out.append(f"| {', '.join('`' + p + '`' for p in patterns)} | {scope} |")
    out += ["| anything else | pr |", "", "`shared`: read by both the engine (or agents, search, ladder) and the "
            "trainer, so a change there is mixed and runs the whole pull request set.", "",
            "## Scope to checks", "", "| scope | ci.yml jobs | what runs |", "|---|---|---|"]
    for name, spec in SCOPES.items():
        out.append(f"| {name} | {', '.join(spec['jobs']) or 'none'} | {spec['summary']} |")
    out += ["", "## Trainer scope", "", "Library test filters: " + ", ".join(f"`{f}`" for f in TRAINER_FILTERS) + ".",
            "", trainer_record(), "", "## Rules and cards", "", RULES_NOTE, ""]
    return "\n".join(out)


def main(argv: list[str]) -> int:
    command = argv[1] if len(argv) > 1 else ""
    if command == "markdown":
        sys.stdout.write(markdown())
        return 0
    if command == "trainer-filters":
        print(" ".join(TRAINER_FILTERS))
        return 0
    if command == "commands":
        scope = argv[2]
        lines = {"tools": TOOLS_COMMANDS,
                 "trainer": ["python python/tools/run_rust_lib_filters.py " + " ".join(TRAINER_FILTERS)]}.get(scope)
        if lines is None:
            print(f"no local command list for scope {scope}", file=sys.stderr)
            return 2
        print("\n".join(lines))
        return 0
    if command == "choose":
        labels = json.loads(os.environ.get("LABELS") or "[]")
        files = [line for line in (os.environ.get("CHANGED_FILES") or "").splitlines() if line.strip()]
        checks = choose(os.environ.get("EVENT", ""), os.environ.get("SCOPE", ""), os.environ.get("INPUT_SCOPE", ""),
                        labels, files)
        lines = [f"checks: {checks}" + ("" if checks == "all" else f" ({SCOPES[checks]['summary']})")]
        if checks == "trainer":
            lines.append(trainer_record())
        print("\n".join(lines))
        for name, text in (("GITHUB_OUTPUT", f"checks={checks}\n"), ("GITHUB_STEP_SUMMARY", "\n\n".join(lines) + "\n")):
            path = os.environ.get(name)
            if path:
                with open(path, "a", encoding="utf-8") as out:
                    out.write(text)
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv))
