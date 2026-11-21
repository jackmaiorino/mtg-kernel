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
  ci_scope.py trainer-filters [--shard N]  print the trainer scope's library test filters (or shard N's).
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
TOOLS_DIRS = ["checkpoint_codec_bench", "native_fixture_bench", "sd1", "search_experiment", "stage4a", "t1_diagnosis"]
TRAINER_ROOTS = ["expanded_deck_training_v1", "native_expanded_training_run_v1", "native_policy_train_step_v1"]
PATH_RULES: list[tuple[tuple[str, ...], str]] = [
    (("data/*",), "pr"),
    (("docs/*", "*.md"), "docs"),
    ((".github/workflows/ci.yml", ".github/actions/*", "tools/ci/*"), "pr"),
    # w5-tools.yml also watches a few files python/tests reads (tools/spy_reset, ex1, two python/tools
    # modules); those run the Python set, and w5-tools.yml still runs on them by its own paths.
    (("tools/w5/*", ".github/workflows/w5-tools.yml"), "w5-tools"),
    ((".github/workflows/*",), "workflow"),
    # Tool directories whose tests all live beside them (no python/tests or python/tools module reads them);
    # every other tools/** directory has tests in python/tests and runs the Python set.
    (tuple(f"tools/{name}/*" for name in TOOLS_DIRS), "tools"),
    (("tools/*",), "python"),
    # Shared: observation encoding, action windows, mulligans, sampling, the policy network, and every part
    # of the trainer that code outside it reads (the module root's inference loaders, the train step's 41
    # outside users, the submodules listed). tools/ci/tests/test_ci_scope.py fails if a trainer file below
    # becomes readable from outside.
    (("mtg-kernel/src/*mulligan*", "mtg-kernel/src/*action_windows*", "mtg-kernel/src/policy_observation_*",
      "mtg-kernel/src/native_flat_tensorizer_*", "mtg-kernel/src/native_policy_value_net_v1*",
      "mtg-kernel/src/fast_sampler.rs", "mtg-kernel/src/unclamped_softmax_sampler_v1.rs",
      "mtg-kernel/src/sideboard_play_policy_v1*", "mtg-kernel/src/rl_session.rs", "mtg-kernel/src/rl_session/*",
      # The whole of expanded_deck_training_v1: its root's inference loaders (human matches, agents, the
      # Spellbench scorer) reach into its submodules through grouped imports and re-exports.
      "mtg-kernel/src/expanded_deck_training_v1.rs", "mtg-kernel/src/expanded_deck_training_v1/*",
      "mtg-kernel/src/native_policy_train_step_v1.rs", "mtg-kernel/src/native_policy_train_step_v1/*"), "shared"),
    (("mtg-kernel/src/native_expanded_training_run_v1.rs", "mtg-kernel/src/native_expanded_training_run_v1/*",
      "mtg-kernel/src/bin/native_expanded_training_run_v1.rs"), "trainer"),
    (("mtg-kernel/src/bin/kernel_limited_*",), "rules-limited"),
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
              "summary": "the unit suites beside the tool directories in TOOLS_DIRS (no Rust binary), Linux"},
    "w5-tools": {"jobs": [],
                 "summary": "nothing in ci.yml: w5-tools.yml runs its Linux (Python 3.11) and Windows unit jobs and "
                            "the cgroup acceptance on these paths"},
    "python": {"jobs": ["python-env-build", "python-tests"],
               "summary": "the Python test binaries and shards, Linux and Windows"},
    "rules-limited": {"jobs": ["rust-quality", "limited-build", "rust-limited-groups"],
                      "summary": "formatting and Clippy, the shared Limited build and every Limited group, Linux "
                                 "(only Limited process binaries map here; Limited library code is engine code)"},
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
    "s_transformer_v1::",
]
# The trainer tests run as two jobs, each building the library and running its filters. Balanced from the
# filter times of hosted run 44906865422 (seconds): expanded_deck_training_v1 408, native_trainer_v1 306,
# native_training_ 187, native_policy_train_step_v1 74, native_expanded_training_run_v1 49,
# native_checkpoint_ 11, the rest under 2 each; about 531 s and 505 s. s_transformer_v1 (the S learner's
# update, added after that run) joins the second.
TRAINER_SHARDS = [
    ["expanded_deck_training_v1::", "native_policy_train_step_v1::", "native_expanded_training_run_v1::"],
    ["native_policy_value_net_v1::", "training_math_v1::", "native_flat_tensorizer_v", "inference_cache_v1::",
     "native_train_state_payload_v1::", "native_trainer_schedule_v", "native_trainer_v1::", "native_training_",
     "native_checkpoint_", "native_baseline_checkpoint_chain_v4::", "fast_sampler::", "unclamped_softmax_sampler_v1::",
     "s_transformer_v1::"],
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
TOOLS_COMMANDS = [f"uv run --no-sync python -m unittest discover -v -s tools/{name}/{sub} -p 'test_*.py'"
                  for name, sub in (("search_experiment", "tests"), ("stage4a", "tests"), ("t1_diagnosis", ""),
                                    ("sd1", "tests"), ("native_fixture_bench", "tests"),
                                    ("checkpoint_codec_bench", "tests"))]
TOOLS_COMMANDS = [c.replace("/ -p", " -p") for c in TOOLS_COMMANDS]


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
    # Labels never narrow a run: paths decide, ci:full widens.
    scope = scope_of_files(files)
    return "all" if scope == "pr" else scope


def trainer_record() -> str:
    rows = "; ".join(BIT_IDENTITY_ROWS)
    return (f"Trainer scope: CI runs formatting, Clippy and the trainer library tests only (a local run covers "
            f"formatting, workspace Clippy and the library tests; the feature Clippy runs stay in the hosted "
            f"rust-quality job). The bit-identity rows "
            f"go with every trainer change and run on a node, not in CI: {rows}, {BIT_IDENTITY_BUILDS}. "
            f"{TRAINER_RULE}")


def markdown() -> str:
    out = ["# CI scopes", "",
           "Generated by `python tools/ci/ci_scope.py markdown` from `tools/ci/ci_scope.py`; "
           "`tools/ci/tests/test_ci_scope.py` fails if this file and the table differ.", "",
           "A ready pull request whose files all map to one scope runs only that scope's checks. Files of more than "
           "one scope, a shared file, or a file no rule names run the whole pull request set. Docs files never "
           "decide. Main pushes, tags and dispatches without `scope` run the full matrix as a monitor: pushes to a "
           "line share one concurrency group (one running, one waiting, a newer merge replaces the waiting one), and "
           "a red monitor's report names every merge it covers. `ci:full` on a pull request forces the whole pull "
           "request set from the next push, reopen or ready-for-review event (adding the label starts no run); "
           "labels never narrow a run. `gh workflow run ci.yml --ref REF -f scope=SCOPE` runs one "
           "scope on any ref.", "",
           "## Path rules (first match decides)", "", "| paths | scope |", "|---|---|"]
    for patterns, scope in PATH_RULES:
        out.append(f"| {', '.join('`' + p + '`' for p in patterns)} | {scope} |")
    out += ["| anything else | pr |", "", "`shared`: read by both the engine (or agents, search, ladder) and the "
            "trainer, so a change there is mixed and runs the whole pull request set.", "",
            "## Scope to checks", "", "| scope | ci.yml jobs | what runs |", "|---|---|---|"]
    for name, spec in SCOPES.items():
        out.append(f"| {name} | {', '.join(spec['jobs']) or 'none'} | {spec['summary']} |")
    out += ["", "## Trainer scope", "", "Library test filters: " + ", ".join(f"`{f}`" for f in TRAINER_FILTERS) + ".",
            "", "They run as " + str(len(TRAINER_SHARDS)) + " jobs: " + "; ".join(
                f"shard {i}: " + ", ".join(f"`{f}`" for f in shard) for i, shard in enumerate(TRAINER_SHARDS)) + ".",
            "", trainer_record(), "", "## Rules and cards", "", RULES_NOTE, ""]
    return "\n".join(out)


def main(argv: list[str]) -> int:
    command = argv[1] if len(argv) > 1 else ""
    if command == "markdown":
        sys.stdout.write(markdown())
        return 0
    if command == "trainer-filters":
        # With --shard N, one shard's filters (the trainer-tests matrix); otherwise all of them.
        print(" ".join(TRAINER_SHARDS[int(argv[3])] if argv[2:3] == ["--shard"] else TRAINER_FILTERS))
        return 0
    if command == "commands":
        scope = argv[2]
        lines = {"tools": TOOLS_COMMANDS,
                 "trainer": ["cargo fmt --all -- --check",
                             "cargo clippy --release --locked --workspace --all-targets -- -D warnings",
                             'python python/tools/run_rust_lib_filters.py --features "" ' + " ".join(TRAINER_FILTERS)]}.get(scope)
        if lines is None:
            print(f"no local command list for scope {scope}", file=sys.stderr)
            return 2
        print("\n".join(lines))
        return 0
    if command == "file-scope":
        print(scope_of_files([line.strip() for line in sys.stdin if line.strip()]))
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
